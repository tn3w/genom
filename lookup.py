"""Reverse geocoding over a genom geo.bin database."""

import math
import struct
import sys
from datetime import datetime
from pathlib import Path
from zoneinfo import ZoneInfo

MAGIC = b"GEO2"
HEADER_WORDS = 17
EARTH_RADIUS_KM = 6371.0
CITY_FIELDS = (
    "city", "city_ascii", "region", "region_code", "district", "district_code",
    "timezone", "country", "feature_code", "population", "elevation", "geoname_id",
)
TEXT_CITY_FIELDS = {
    "city", "city_ascii", "region", "region_code", "district", "district_code",
    "timezone", "feature_code",
}
COUNTRY_FIELDS = (
    "code", "iso3", "iso_numeric", "name", "capital", "area_km2", "population",
    "continent_code", "continent_name", "tld", "currency", "currency_name",
    "phone_prefix", "postal_code_format", "languages", "neighbours",
)


def unzigzag(value):
    return (value >> 1) ^ -(value & 1)


def wrap_degrees(degrees):
    return (degrees + 540) % 360 - 180


def haversine(latitude, longitude, other_latitude, other_longitude):
    latitude_delta = math.radians(other_latitude - latitude) / 2
    longitude_delta = math.radians(wrap_degrees(other_longitude - longitude)) / 2
    chord = math.sin(latitude_delta) ** 2 + (
        math.cos(math.radians(latitude))
        * math.cos(math.radians(other_latitude))
        * math.sin(longitude_delta) ** 2
    )
    return 2 * EARTH_RADIUS_KM * math.asin(math.sqrt(chord))


def ring(radius):
    if radius == 0:
        return [(0, 0)]
    horizontal = [(row, step) for step in range(-radius, radius + 1)
                  for row in (-radius, radius)]
    vertical = [(step, column) for step in range(1 - radius, radius)
                for column in (-radius, radius)]
    return horizontal + vertical


class Layer:
    """A grid of points whose records carry a fixed number of integer fields."""

    def __init__(self, data, header, max_degrees):
        self.data = data
        self.count, self.directory, self.body, self.cells_per_degree, self.fields = header
        self.rows = 180 * self.cells_per_degree
        self.columns = 360 * self.cells_per_degree
        self.max_degrees = max_degrees

    def cell(self, row, column):
        if not 0 <= row < self.rows:
            return None
        cell = row * self.columns + column % self.columns
        low, high = 0, self.count
        while low < high:
            middle = (low + high) // 2
            entry = self.directory + 8 * middle
            key, offset = struct.unpack_from("<2I", self.data, entry)
            if key == cell:
                return self.body + offset
            if key < cell:
                low = middle + 1
            else:
                high = middle
        return None

    def records(self, position):
        data = self.data

        def varint():
            nonlocal position
            value = shift = 0
            while True:
                byte = data[position]
                position += 1
                value |= (byte & 0x7F) << shift
                if byte < 0x80:
                    return value
                shift += 7

        latitude = longitude = 0
        for _ in range(varint()):
            latitude += unzigzag(varint())
            longitude += unzigzag(varint())
            fields = [varint() for _ in range(self.fields)]
            yield latitude / 1e6, longitude / 1e6, fields

    def nearest(self, latitude, longitude, accept=lambda fields: True):
        row_position = (latitude + 90) * self.cells_per_degree
        column_position = (longitude + 180) * self.cells_per_degree
        row, column = int(row_position), int(column_position)
        edge = lambda position: min(position % 1, 1 - position % 1)
        longitude_scale = math.cos(math.radians(latitude))
        row_limit = math.ceil(self.max_degrees * self.cells_per_degree)
        max_radius = int(row_limit / max(longitude_scale, 0.2))
        best, best_distance = None, self.max_degrees ** 2
        for radius in range(max_radius + 1):
            row_reach = radius - 1 + edge(row_position)
            column_reach = (radius - 1 + edge(column_position)) * longitude_scale
            reach = min(row_reach, column_reach) / self.cells_per_degree
            if radius > 0 and reach * reach > best_distance:
                break
            for row_step, column_step in ring(radius):
                if abs(row_step) > row_limit:
                    continue
                position = self.cell(row + row_step, column + column_step)
                if position is None:
                    continue
                for record in self.records(position):
                    if not accept(record[2]):
                        continue
                    latitude_delta = record[0] - latitude
                    longitude_delta = wrap_degrees(record[1] - longitude)
                    longitude_delta *= longitude_scale
                    distance = latitude_delta ** 2 + longitude_delta ** 2
                    if distance < best_distance:
                        best, best_distance = record, distance
        return best


class Geo:
    """Reverse geocoder backed by a geo.bin database."""

    def __init__(self, path):
        data = Path(path).read_bytes()
        if data[:4] != MAGIC:
            raise ValueError(f"{path} is not a geo.bin file")
        self.data = data
        self.header = struct.unpack_from(f"<{HEADER_WORDS}I", data)
        self.cities = Layer(data, self.header[7:12], max_degrees=10)
        self.postal = Layer(data, self.header[12:17], max_degrees=0.3)

    def text(self, index):
        _, count, offsets, body = self.header[:4]
        if index >= count:
            return ""
        start, end = struct.unpack_from("<2I", self.data, offsets + 4 * index)
        return self.data[body + start:body + end].decode()

    def country_field(self, country, column):
        columns, table = self.header[5:7]
        position = table + 4 * (country * columns + column)
        return struct.unpack_from("<I", self.data, position)[0]

    def country(self, country):
        values = {name: self.text(self.country_field(country, column))
                  for column, name in enumerate(COUNTRY_FIELDS)}
        values["area_km2"] = float(values["area_km2"] or 0)
        values["population"] = int(values["population"] or 0)
        for name in ("languages", "neighbours"):
            values[name] = [item for item in values[name].split(",") if item]
        values["is_eu"] = self.country_field(country, 16) == 1
        return values

    def lookup(self, latitude, longitude):
        """Return the nearest place as a dict, or None when nothing is in range."""
        city = self.cities.nearest(latitude, longitude)
        if city is None:
            return None
        city_latitude, city_longitude, fields = city
        place = dict(zip(CITY_FIELDS, fields))
        country = place.pop("country")
        for name in TEXT_CITY_FIELDS:
            place[name] = self.text(place[name])
        elevation = place["elevation"]
        place["elevation"] = unzigzag(elevation - 1) if elevation else None
        place.update(postal_details(self, country, latitude, longitude))
        place.update(
            latitude=city_latitude,
            longitude=city_longitude,
            distance_km=haversine(latitude, longitude, city_latitude, city_longitude),
            timezone=timezone_details(place["timezone"]),
            country=self.country(country),
        )
        return place


def postal_details(geo, country, latitude, longitude):
    match = None
    if geo.country_field(country, 17):
        same_country = lambda fields: fields[1] == country
        match = geo.postal.nearest(latitude, longitude, same_country)
    code, _, locality, municipality = match[2] if match else (0, 0, 0, 0)
    return {
        "postal_code": geo.text(code),
        "locality": geo.text(locality),
        "municipality": geo.text(municipality),
    }


def format_offset(seconds):
    hours, remainder = divmod(abs(seconds), 3600)
    sign = "-" if seconds < 0 else "+"
    minutes = remainder // 60
    return f"UTC{sign}{hours}" + (f":{minutes:02}" if minutes else "")


def timezone_details(name):
    try:
        now = datetime.now(ZoneInfo(name))
    except (KeyError, ValueError):
        return {"name": name, "abbreviation": "", "utc_offset": 0,
                "utc_offset_str": format_offset(0), "dst_active": False}
    offset = int(now.utcoffset().total_seconds())
    return {
        "name": name,
        "abbreviation": now.tzname() or "",
        "utc_offset": offset,
        "utc_offset_str": format_offset(offset),
        "dst_active": bool(now.dst()),
    }


if __name__ == "__main__":
    from pprint import pprint

    database = sys.argv[3] if len(sys.argv) > 3 else "geo.bin"
    pprint(Geo(database).lookup(float(sys.argv[1]), float(sys.argv[2])), sort_dicts=False)
