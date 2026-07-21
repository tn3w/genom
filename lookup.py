"""Reverse geocoding over a pre-built geo.bin index."""

from datetime import datetime
from pathlib import Path
from typing import NamedTuple
from zoneinfo import ZoneInfo

MAGIC = b"GEO1"
CITY_FIELD_COUNT = 6
POSTAL_FIELD_COUNT = 2
MICRODEGREES = 1_000_000


class Grid(NamedTuple):
    scale: int
    rows: int
    columns: int
    max_radius: int


class Country(NamedTuple):
    name: str
    currency: str
    continent: str
    in_european_union: bool


CITY_GRID = Grid(scale=10, rows=1800, columns=3600, max_radius=200)
POSTAL_GRID = Grid(scale=100, rows=18000, columns=36000, max_radius=1000)
UNKNOWN_COUNTRY = Country("Unknown", "", "", False)


def read_uint16(data, position):
    return int.from_bytes(data[position:position + 2], "little")


def read_uint32(data, position):
    return int.from_bytes(data[position:position + 4], "little")


def read_varint(data, position):
    value = shift = 0
    while True:
        byte = data[position]
        position += 1
        value |= (byte & 0x7F) << shift
        if byte < 0x80:
            return value, position
        shift += 7


def read_zigzag(data, position):
    value, position = read_varint(data, position)
    return (value >> 1) ^ -(value & 1), position


def read_slice(data, offsets, body, index):
    start = read_uint32(data, offsets + 4 * index)
    end = read_uint32(data, offsets + 4 * index + 4)
    return data[body + start:body + end].decode()


def read_cell_directory(data, position, count):
    cells = {}
    cell = offset = 0
    for _ in range(count):
        cell_delta, position = read_varint(data, position)
        offset_delta, position = read_varint(data, position)
        cell += cell_delta
        offset += offset_delta
        cells[cell] = offset
    return cells, position


def read_records(data, position, field_count):
    count, position = read_varint(data, position)
    latitude = longitude = 0
    records = []
    for _ in range(count):
        latitude_delta, position = read_zigzag(data, position)
        longitude_delta, position = read_zigzag(data, position)
        latitude += latitude_delta
        longitude += longitude_delta
        fields = []
        for _ in range(field_count):
            field, position = read_varint(data, position)
            fields.append(field)
        records.append((latitude, longitude, fields))
    return records


def containing_cell(grid, latitude, longitude):
    row = min(int((latitude + 90) * grid.scale), grid.rows - 1)
    column = min(int((longitude + 180) * grid.scale), grid.columns - 1)
    return max(row, 0), max(column, 0)


def ring_offsets(radius):
    if radius == 0:
        yield 0, 0
        return
    for offset in range(-radius, radius + 1):
        yield -radius, offset
        yield radius, offset
    for offset in range(1 - radius, radius):
        yield offset, -radius
        yield offset, radius


def ring_records(grid, cells, load_records, row, column, radius):
    for row_offset, column_offset in ring_offsets(radius):
        cell_row = row + row_offset
        cell_column = column + column_offset
        if not (0 <= cell_row < grid.rows and 0 <= cell_column < grid.columns):
            continue
        offset = cells.get(cell_row * grid.columns + cell_column)
        if offset is not None:
            yield from load_records(offset)


def squared_distance(record, latitude, longitude):
    return (record[0] - latitude) ** 2 + (record[1] - longitude) ** 2


def ring_cannot_improve(grid, radius, best_distance):
    closest_possible = (radius - 1) * MICRODEGREES / grid.scale
    return radius > 1 and closest_possible ** 2 > best_distance


def nearest_record(grid, cells, load_records, latitude, longitude):
    if not cells:
        return None
    target_latitude = int(latitude * MICRODEGREES)
    target_longitude = int(longitude * MICRODEGREES)
    row, column = containing_cell(grid, latitude, longitude)
    best = None
    best_distance = 0
    for radius in range(grid.max_radius + 1):
        if best is not None and ring_cannot_improve(grid, radius, best_distance):
            break
        for record in ring_records(grid, cells, load_records, row, column, radius):
            distance = squared_distance(record, target_latitude, target_longitude)
            if best is None or distance < best_distance:
                best, best_distance = record, distance
    return best


class PostalCountry:
    """Postal code strings and their spatial index for one country."""

    def __init__(self, data, start):
        self.data = data
        position = start + 4 + read_uint32(data, start) * 12
        count = read_uint32(data, position)
        self.code_offsets = position + 4
        self.code_body = self.code_offsets + 4 * (count + 1)
        position = self.code_body + read_uint32(data, self.code_offsets + 4 * count)
        cell_count, position = read_varint(data, position)
        self.cells, self.body = read_cell_directory(data, position, cell_count)

    def code(self, index):
        return read_slice(self.data, self.code_offsets, self.code_body, index)

    def records(self, offset):
        return read_records(self.data, self.body + offset, POSTAL_FIELD_COUNT)


def read_postal_index(data, index, base):
    countries = {}
    position = index + 4
    for _ in range(read_uint32(data, index)):
        start = base + read_uint32(data, position + 4)
        countries[read_uint16(data, position)] = PostalCountry(data, start)
        position += 12
    return countries


class Geo:
    """Reverse geocoder backed by a geo.bin index file."""

    def __init__(self, path):
        data = Path(path).read_bytes()
        if data[:4] != MAGIC:
            raise ValueError(f"{path} is not a geo.bin file")
        self.data = data
        strings, codes, grid, cities, postal_index, postal = (
            read_uint32(data, position) for position in (8, 16, 24, 32, 40, 48)
        )
        self.string_count = read_uint32(data, strings)
        self.string_offsets = strings + 4
        self.string_body = self.string_offsets + 4 * (self.string_count + 1)
        self.country_codes = codes + 4
        self.city_body = cities
        self.city_cells, _ = read_cell_directory(data, grid + 4, read_uint32(data, grid))
        self.postal_countries = read_postal_index(data, postal_index, postal)

    def lookup(self, latitude, longitude):
        """Return an enriched place mapping, or None when no city is in range."""
        city = nearest_record(
            CITY_GRID, self.city_cells, self.city_records, latitude, longitude
        )
        if city is None:
            return None
        city_latitude, city_longitude, fields = city
        name, region, district, region_code, timezone = map(self.text, fields[:5])
        country_index = fields[5]
        return build_place(
            city=name,
            region=region,
            region_code=region_code,
            district=district,
            country_code=self.country_code(country_index),
            postal_code=self.postal_code(country_index, latitude, longitude),
            timezone=timezone,
            latitude=city_latitude / MICRODEGREES,
            longitude=city_longitude / MICRODEGREES,
        )

    def city_records(self, offset):
        return read_records(self.data, self.city_body + offset, CITY_FIELD_COUNT)

    def postal_code(self, country_index, latitude, longitude):
        country = self.postal_countries.get(country_index)
        if country is None:
            return ""
        match = nearest_record(
            POSTAL_GRID, country.cells, country.records, latitude, longitude
        )
        if match is None:
            return ""
        return country.code(match[2][0])

    def text(self, index):
        if index >= self.string_count:
            return ""
        return read_slice(self.data, self.string_offsets, self.string_body, index)

    def country_code(self, index):
        start = self.country_codes + 2 * index
        return self.data[start:start + 2].decode()


def offset_seconds(moment):
    offset = moment.utcoffset()
    return 0 if offset is None else int(offset.total_seconds())


def timezone_details(name):
    try:
        zone = ZoneInfo(name)
    except (KeyError, ValueError):
        return "", 0, False
    now = datetime.now(zone)
    offset = offset_seconds(now)
    winter = offset_seconds(datetime(2024, 1, 15, 12, tzinfo=zone))
    summer = offset_seconds(datetime(2024, 7, 15, 12, tzinfo=zone))
    return now.tzname() or "", offset, offset != min(winter, summer)


def format_utc_offset(seconds):
    hours, remainder = divmod(abs(seconds), 3600)
    sign = "-" if seconds < 0 else "+"
    minutes = remainder // 60
    return f"UTC{sign}{hours}" + (f":{minutes:02}" if minutes else "")


def build_place(city, region, region_code, district, country_code, postal_code,
                timezone, latitude, longitude):
    abbreviation, utc_offset, dst_active = timezone_details(timezone)
    country = COUNTRIES.get(country_code, UNKNOWN_COUNTRY)
    return {
        "city": city,
        "region": region,
        "region_code": region_code,
        "district": district,
        "country_code": country_code,
        "country_name": country.name,
        "postal_code": postal_code,
        "timezone": timezone,
        "timezone_abbr": abbreviation,
        "utc_offset": utc_offset,
        "utc_offset_str": format_utc_offset(utc_offset),
        "latitude": latitude,
        "longitude": longitude,
        "currency": country.currency,
        "continent_code": CONTINENT_CODES.get(country.continent, ""),
        "continent_name": country.continent or "Unknown",
        "is_eu": country.in_european_union,
        "dst_active": dst_active,
    }


CONTINENT_CODES = {"Africa": "AF", "Antarctica": "AN", "Asia": "AS",
                   "Europe": "EU", "North America": "NA", "Oceania": "OC",
                   "South America": "SA"}

CONTINENT_NAMES = {"F": "Africa", "S": "Asia", "E": "Europe", "e": "Europe",
                   "N": "North America", "U": "South America", "O": "Oceania",
                   "Q": "Antarctica", "-": None}

SHARED_CURRENCIES = "EUR USD XCD XOF AUD XAF NZD XPF NOK DKK - RSD MAD GBP CHF ILS".split()

COUNTRY_DATA = """
ADE0Andorra AESdUnited_Arab_Emirates AFSnAfghanistan AGN2Antigua_and_Barbuda AIN2Anguilla
ALElAlbania AMSdArmenia ANNgNetherlands_Antilles AOFaAngola AQQ1Antarctica ARUsArgentina
ASO1American_Samoa ATe0Austria AUOdAustralia AW-gAruba AZSnAzerbaijan BAEmBosnia_and_Herzegovina
BBNdBarbados BDStBangladesh BEe0Belgium BFF3Burkina_Faso BGenBulgaria BHSdBahrain BIFfBurundi
BJF3Benin BMNdBermuda BNSdBrunei BOUbBolivia BRUlBrazil BSNdBahamas BTSnBhutan BVQ8Bouvet_Island
BWFpBotswana BYErBelarus BZNdBelize CANdCanada CCS4Cocos_(Keeling)_Islands
CDFfDemocratic_Republic_of_the_Congo CFF5Central_African_Republic CGF5Republic_of_the_Congo
CHEfSwitzerland CIF3Ivory_Coast CKO6Cook_Islands CLUpChile CMF5Cameroon CNSyChina COUpColombia
CRNcCosta_Rica CSEBSerbia_and_Montenegro CUNpCuba CVFeCape_Verde CXS4Christmas_Island CYe0Cyprus
CZekCzech_Republic DEe0Germany DJFfDjibouti DKekDenmark DMN2Dominica DONpDominican_Republic
DZFdAlgeria ECU1Ecuador EEe0Estonia EGFpEgypt EHFCWestern_Sahara ERFnEritrea ESe0Spain
ETFbEthiopia FIe0Finland FJOdFiji FKUpFalkland_Islands FMO1Micronesia FO-9Faroe_Islands
FRe0France GAF5Gabon GBEpUnited_Kingdom GDN2Grenada GESlGeorgia GFU0French_Guiana GHFsGhana
GIEpGibraltar GLN9Greenland GMFdGambia GNFfGuinea GPN0Guadeloupe GQF5Equatorial_Guinea
GRe0Greece GSQDSouth_Georgia_and_the_South_Sandwich_Islands GTNqGuatemala GUO1Guam
GWF3Guinea-Bissau GYUdGuyana HKSdHong_Kong HMQ4Heard_Island_and_McDonald_Islands HNNlHonduras
HRekCroatia HTNgHaiti HUefHungary IDSrIndonesia IEe0Ireland ILSsIsrael INSrIndia
IOS1British_Indian_Ocean_Territory IQSdIraq IRSrIran ISEkIceland ITe0Italy JMNdJamaica
JOSdJordan JPSyJapan KEFsKenya KGSsKyrgyzstan KHSrCambodia KIO4Kiribati KMFfComoros
KNN2Saint_Kitts_and_Nevis KPSwNorth_Korea KRSwSouth_Korea KWSdKuwait KYNdCayman_Islands
KZStKazakhstan LASkLaos LBSpLebanon LCN2Saint_Lucia LIEELiechtenstein LKSrSri_Lanka LRFdLiberia
LSFlLesotho LTe0Lithuania LUe0Luxembourg LVe0Latvia LYFdLibya MAFdMorocco MCE0Monaco MDElMoldova
MEEAMontenegro MGFaMadagascar MHO1Marshall_Islands MKEdMacedonia MLF3Mali MMSkMyanmar
MNStMongolia MOSpMacau MPO1Northern_Mariana_Islands MQN0Martinique MRFuMauritania MSN2Montserrat
MTe0Malta MUFrMauritius MVSrMaldives MWFkMalawi MXNnMexico MYSrMalaysia MZFnMozambique
NAFdNamibia NCO7New_Caledonia NEF3Niger NFO4Norfolk_Island NGFnNigeria NINoNicaragua
NLe0Netherlands NOEkNorway NPSrNepal NRO4Nauru NUO6Niue NZOdNew_Zealand OMSrOman PANbPanama
PEUnPeru PFO7French_Polynesia PGOkPapua_New_Guinea PHSpPhilippines PKSrPakistan PLenPoland
PMN0Saint_Pierre_and_Miquelon PNO6Pitcairn PRN1Puerto_Rico PSSFPalestinian_Territory
PTe0Portugal PWO1Palau PYUgParaguay QASrQatar RE-0Reunion ROenRomania RSEASerbia RUEbRussia
RWFfRwanda SASrSaudi_Arabia SBOdSolomon_Islands SCFrSeychelles SDFgSudan SEekSweden
SGSdSingapore SH-pSaint_Helena SIe0Slovenia SJE8Svalbard_and_Jan_Mayen SKe0Slovakia
SLFlSierra_Leone SME0San_Marino SNF3Senegal SOFsSomalia SRUdSuriname STFnSão_Tomé_and_Príncipe
SVNcEl_Salvador SYSpSyria SZFlSwaziland TCN1Turks_and_Caicos_Islands TDF5Chad
TFQ0French_Southern_Territories TGF3Togo THSbThailand TJSsTajikistan TKO6Tokelau TLS1East_Timor
TMStTurkmenistan TNFdTunisia TOOpTonga TRSyTurkey TT-dTrinidad_and_Tobago TVO4Tuvalu TWSdTaiwan
TZFsTanzania UAEhUkraine UGFxUganda UMN1United_States_Minor_Outlying_Islands USNdUnited_States
UYUuUruguay UZSsUzbekistan VAE0Vatican_City VCN2Saint_Vincent_and_the_Grenadines VEUsVenezuela
VG-1British_Virgin_Islands VIN1U.S._Virgin_Islands VNSdVietnam VUOvVanuatu WFO7Wallis_and_Futuna
WSOtSamoa YESrYemen YT-0Mayotte ZAFrSouth_Africa ZMFwZambia ZWFlZimbabwe"""


def parse_currency(country_code, symbol):
    if symbol.islower():
        return country_code + symbol.upper()
    index = int(symbol) if symbol.isdigit() else 10 + ord(symbol) - ord("A")
    currency = SHARED_CURRENCIES[index]
    return "" if currency == "-" else currency


def parse_countries(data):
    countries = {}
    for token in data.split():
        code, marker, symbol = token[:2], token[2], token[3]
        countries[code] = Country(
            name=token[4:].replace("_", " "),
            currency=parse_currency(code, symbol),
            continent=CONTINENT_NAMES[marker] or "",
            in_european_union=marker == "e",
        )
    return countries


COUNTRIES = parse_countries(COUNTRY_DATA)
