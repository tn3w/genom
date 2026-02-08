const DB_URL = 
  'https://raw.githubusercontent.com/tn3w/genom/master/docs-src/places.bin.xz';
const DB_NAME = 'genom-db';
const DB_VERSION = 1;
const STORE_NAME = 'places';

class GenomDB {
  constructor() {
    this.ready = false;
    this.status = 'initializing';
    this.progress = 0;
    this.listeners = [];
    this.wasm = null;
  }

  onStatusChange(callback) {
    this.listeners.push(callback);
    callback(this.status, this.progress);
  }

  notifyListeners() {
    this.listeners.forEach(cb => cb(this.status, this.progress));
  }

  async init() {
    try {
      this.status = 'loading wasm';
      this.notifyListeners();
      await this.loadWasm();

      const cached = await this.getCachedData();
      if (cached) {
        this.status = 'loading cached';
        this.notifyListeners();
        await this.initGeocoder(cached);
        this.ready = true;
        this.status = 'ready';
        this.progress = 100;
        this.notifyListeners();
        return;
      }

      this.status = 'downloading';
      this.notifyListeners();
      const compressed = await this.downloadWithProgress();

      this.status = 'decompressing';
      this.progress = 0;
      this.notifyListeners();
      const decompressed = await this.decompressAsync(compressed);

      await this.cacheData(decompressed);
      await this.initGeocoder(decompressed);

      this.ready = true;
      this.status = 'ready';
      this.progress = 100;
      this.notifyListeners();
    } catch (error) {
      this.status = `error: ${error.message}`;
      this.notifyListeners();
      throw error;
    }
  }

  async loadWasm() {
    this.wasm = await import('./genom_wasm.js');
    await this.wasm.default();
  }

  async downloadWithProgress() {
    const response = await fetch(DB_URL);
    if (!response.ok) throw new Error('Download failed');

    const total = parseInt(response.headers.get('content-length') || '0');
    const reader = response.body.getReader();
    const chunks = [];
    let received = 0;

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      chunks.push(value);
      received += value.length;

      if (total > 0) {
        this.progress = Math.round((received / total) * 100);
        this.notifyListeners();
      }
    }

    return new Uint8Array(
      chunks.reduce((acc, chunk) => [...acc, ...chunk], [])
    );
  }

  async decompressAsync(compressed) {
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        try {
          this.progress = 10;
          this.notifyListeners();
          
          const decompressed = this.wasm.decompress_xz(compressed);
          
          this.progress = 95;
          this.notifyListeners();
          
          resolve(decompressed);
        } catch (error) {
          reject(error);
        }
      }, 0);
    });
  }

  async initGeocoder(data) {
    this.wasm.init_geocoder(data);
  }

  async getCachedData() {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(DB_NAME, DB_VERSION);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const db = request.result;
        const tx = db.transaction(STORE_NAME, 'readonly');
        const store = tx.objectStore(STORE_NAME);
        const get = store.get('data');

        get.onsuccess = () => resolve(get.result);
        get.onerror = () => resolve(null);
      };

      request.onupgradeneeded = (event) => {
        const db = event.target.result;
        if (!db.objectStoreNames.contains(STORE_NAME)) {
          db.createObjectStore(STORE_NAME);
        }
      };
    });
  }

  async cacheData(data) {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(DB_NAME, DB_VERSION);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const db = request.result;
        const tx = db.transaction(STORE_NAME, 'readwrite');
        const store = tx.objectStore(STORE_NAME);
        store.put(data, 'data');

        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
      };
    });
  }

  async lookup(lat, lon) {
    if (!this.ready) throw new Error('DB not ready');
    return this.wasm.lookup(lat, lon);
  }
}

export const genomDB = new GenomDB();
