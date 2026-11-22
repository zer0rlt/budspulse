// Biquad (RBJ Audio-EQ-Cookbook) magnitude response — mirrors src-tauri/src/audio_eq/mod.rs
// so the graph is drawn locally at 60 fps while dragging nodes.
import type { FilterType, HostEqProfile, PeqBand } from "@/types/earbuds";

export const FS = 48000;
export const F_MIN = 20;
export const F_MAX = 20000;

interface Coeffs {
  b0: number;
  b1: number;
  b2: number;
  a0: number;
  a1: number;
  a2: number;
}

export function coeffs(type: FilterType, f0: number, gain: number, q: number, fs = FS): Coeffs {
  const A = Math.pow(10, gain / 40);
  const w0 = (2 * Math.PI * Math.min(f0, fs * 0.49)) / fs;
  const cw = Math.cos(w0);
  const alpha = Math.sin(w0) / (2 * Math.max(q, 0.01));
  const sA = 2 * Math.sqrt(A) * alpha;
  switch (type) {
    case "PK":
      return { b0: 1 + alpha * A, b1: -2 * cw, b2: 1 - alpha * A, a0: 1 + alpha / A, a1: -2 * cw, a2: 1 - alpha / A };
    case "LSC":
      return {
        b0: A * (A + 1 - (A - 1) * cw + sA),
        b1: 2 * A * (A - 1 - (A + 1) * cw),
        b2: A * (A + 1 - (A - 1) * cw - sA),
        a0: A + 1 + (A - 1) * cw + sA,
        a1: -2 * (A - 1 + (A + 1) * cw),
        a2: A + 1 + (A - 1) * cw - sA,
      };
    case "HSC":
      return {
        b0: A * (A + 1 + (A - 1) * cw + sA),
        b1: -2 * A * (A - 1 + (A + 1) * cw),
        b2: A * (A + 1 + (A - 1) * cw - sA),
        a0: A + 1 - (A - 1) * cw + sA,
        a1: 2 * (A - 1 - (A + 1) * cw),
        a2: A + 1 - (A - 1) * cw - sA,
      };
    case "BP":
      return { b0: alpha, b1: 0, b2: -alpha, a0: 1 + alpha, a1: -2 * cw, a2: 1 - alpha };
    case "NO":
      return { b0: 1, b1: -2 * cw, b2: 1, a0: 1 + alpha, a1: -2 * cw, a2: 1 - alpha };
  }
}

export function magnitudeDb(c: Coeffs, f: number, fs = FS): number {
  const w = (2 * Math.PI * f) / fs;
  const cos1 = Math.cos(w);
  const sin1 = Math.sin(w);
  const cos2 = Math.cos(2 * w);
  const sin2 = Math.sin(2 * w);
  const nr = c.b0 + c.b1 * cos1 + c.b2 * cos2;
  const ni = -(c.b1 * sin1 + c.b2 * sin2);
  const dr = c.a0 + c.a1 * cos1 + c.a2 * cos2;
  const di = -(c.a1 * sin1 + c.a2 * sin2);
  const mag2 = (nr * nr + ni * ni) / (dr * dr + di * di);
  return 10 * Math.log10(Math.max(mag2, 1e-12));
}

export function bandResponse(b: PeqBand, f: number): number {
  if (!b.enabled) return 0;
  return magnitudeDb(coeffs(b.type, b.freq, b.gain, b.q), f);
}

export function logFreqs(n = 160): number[] {
  const out: number[] = [];
  const l0 = Math.log10(F_MIN);
  const l1 = Math.log10(F_MAX);
  for (let i = 0; i < n; i++) out.push(Math.pow(10, l0 + ((l1 - l0) * i) / (n - 1)));
  return out;
}

export function totalResponse(p: HostEqProfile, freqs: number[]): number[] {
  return freqs.map((f) => p.preamp + p.bands.reduce((acc, b) => acc + bandResponse(b, f), 0));
}

/** Preamp that prevents 0 dBFS clipping: -(max positive boost of the summed curve). */
export function autoPreamp(p: HostEqProfile): number {
  const freqs = logFreqs(256);
  let peak = 0;
  for (const f of freqs) {
    const v = p.bands.reduce((acc, b) => acc + bandResponse(b, f), 0);
    if (v > peak) peak = v;
  }
  return -Math.round(peak * 10) / 10;
}

export const DEFAULT_BAND_FREQS = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];

export function flatProfile(name = "Flat"): HostEqProfile {
  return {
    name,
    enabled: false,
    preamp: 0,
    autoPreamp: true,
    bands: DEFAULT_BAND_FREQS.map((freq, i) => ({
      enabled: true,
      type: i === 0 ? "LSC" : i === DEFAULT_BAND_FREQS.length - 1 ? "HSC" : "PK",
      freq,
      gain: 0,
      q: i === 0 || i === DEFAULT_BAND_FREQS.length - 1 ? 0.7 : 1.41,
    })),
  };
}

export function fmtFreq(f: number): string {
  return f >= 1000 ? `${(f / 1000).toFixed(f >= 10000 ? 0 : 1)}k` : `${Math.round(f)}`;
}
