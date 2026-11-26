import "@testing-library/jest-dom/vitest";

// Minimal DOMMatrix polyfill for pdfjs-dist when loaded under jsdom in Vitest.
if (typeof globalThis.DOMMatrix === "undefined") {
  class DOMMatrixPolyfill {
    a = 1;
    b = 0;
    c = 0;
    d = 1;
    e = 0;
    f = 0;
    m11 = 1;
    m12 = 0;
    m21 = 0;
    m22 = 1;
    m41 = 0;
    m42 = 0;
    is2D = true;

    constructor(init?: number[] | string) {
      if (Array.isArray(init) && init.length >= 6) {
        this.a = this.m11 = init[0] ?? 1;
        this.b = this.m12 = init[1] ?? 0;
        this.c = this.m21 = init[2] ?? 0;
        this.d = this.m22 = init[3] ?? 1;
        this.e = this.m41 = init[4] ?? 0;
        this.f = this.m42 = init[5] ?? 0;
      }
    }

    multiplySelf() {
      return this;
    }

    preMultiplySelf() {
      return this;
    }

    translateSelf(tx = 0, ty = 0) {
      this.e += tx;
      this.f += ty;
      this.m41 = this.e;
      this.m42 = this.f;
      return this;
    }

    scaleSelf(sx = 1, sy = sx) {
      this.a *= sx;
      this.d *= sy;
      this.m11 = this.a;
      this.m22 = this.d;
      return this;
    }

    invertSelf() {
      return this;
    }
  }

  Object.defineProperty(globalThis, "DOMMatrix", {
    value: DOMMatrixPolyfill,
    writable: true,
    configurable: true,
  });
}
