import { useMemo, useRef, useState } from "react";
import { bandResponse, F_MAX, F_MIN, fmtFreq, logFreqs, totalResponse } from "@/lib/biquad";
import type { HostEqProfile } from "@/types/earbuds";

const W = 340;
const H = 170;
const PAD_L = 24;
const PAD_B = 16;
const DB = 15;

const fx = (f: number) => PAD_L + ((Math.log10(f) - Math.log10(F_MIN)) / (Math.log10(F_MAX) - Math.log10(F_MIN))) * (W - PAD_L - 4);
const xf = (x: number) => Math.pow(10, Math.log10(F_MIN) + ((x - PAD_L) / (W - PAD_L - 4)) * (Math.log10(F_MAX) - Math.log10(F_MIN)));
const dy = (db: number) => 6 + ((DB - Math.max(-DB, Math.min(DB, db))) / (2 * DB)) * (H - PAD_B - 12);
const yd = (y: number) => DB - ((y - 6) / (H - PAD_B - 12)) * 2 * DB;

export default function PeqGraph({
  profile,
  selected,
  onSelect,
  onChange,
  gainLimit = 20,
}: {
  profile: HostEqProfile;
  selected: number;
  onSelect: (i: number) => void;
  onChange: (p: HostEqProfile) => void;
  gainLimit?: number;
}) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const freqs = useMemo(() => logFreqs(180), []);
  const total = useMemo(() => totalResponse(profile, freqs), [profile, freqs]);
  const sel = profile.bands[selected];
  const selCurve = useMemo(() => (sel ? freqs.map((f) => bandResponse(sel, f)) : []), [sel, freqs]);

  const path = (vals: number[]) => vals.map((v, i) => `${i ? "L" : "M"}${fx(freqs[i]).toFixed(1)},${dy(v).toFixed(1)}`).join("");

  const toLocal = (e: React.PointerEvent) => {
    const r = svgRef.current!.getBoundingClientRect();
    return { x: ((e.clientX - r.left) / r.width) * W, y: ((e.clientY - r.top) / r.height) * H };
  };

  const update = (i: number, patch: Partial<HostEqProfile["bands"][number]>) => {
    const bands = profile.bands.map((b, j) => (j === i ? { ...b, ...patch } : b));
    onChange({ ...profile, bands });
  };

  const gridF = [50, 100, 200, 500, 1000, 2000, 5000, 10000];

  return (
    <svg
      ref={svgRef}
      viewBox={`0 0 ${W} ${H}`}
      className="w-full touch-none select-none"
      onPointerMove={(e) => {
        if (drag === null) return;
        const { x, y } = toLocal(e);
        const b = profile.bands[drag];
        const noGain = b.type === "BP" || b.type === "NO";
        update(drag, {
          freq: Math.round(Math.max(F_MIN, Math.min(F_MAX, xf(x)))),
          gain: noGain ? b.gain : Math.round(Math.max(-gainLimit, Math.min(gainLimit, yd(y))) * 10) / 10,
        });
      }}
      onPointerUp={() => setDrag(null)}
      onPointerLeave={() => setDrag(null)}
    >
      {/* grid */}
      {gridF.map((f) => (
        <g key={f}>
          <line x1={fx(f)} x2={fx(f)} y1={6} y2={H - PAD_B} stroke="rgb(var(--md-outline-variant))" strokeWidth={0.5} />
          <text x={fx(f)} y={H - 4} fontSize={7.5} textAnchor="middle" fill="rgb(var(--md-on-surface-variant))">
            {fmtFreq(f)}
          </text>
        </g>
      ))}
      {[-12, -6, 0, 6, 12].map((d) => (
        <g key={d}>
          <line
            x1={PAD_L}
            x2={W - 4}
            y1={dy(d)}
            y2={dy(d)}
            stroke="rgb(var(--md-outline-variant))"
            strokeWidth={d === 0 ? 1 : 0.5}
            strokeDasharray={d === 0 ? "" : "2 3"}
          />
          <text x={PAD_L - 4} y={dy(d) + 2.5} fontSize={7.5} textAnchor="end" fill="rgb(var(--md-on-surface-variant))">
            {d > 0 ? `+${d}` : d}
          </text>
        </g>
      ))}
      {/* selected band area */}
      {sel && sel.enabled && (
        <path d={`${path(selCurve)}L${fx(F_MAX)},${dy(0)}L${fx(F_MIN)},${dy(0)}Z`} fill="rgb(var(--md-primary) / 0.12)" stroke="none" />
      )}
      {/* total curve */}
      <path d={path(total)} fill="none" stroke="rgb(var(--md-primary))" strokeWidth={2} opacity={profile.enabled ? 1 : 0.45} />
      {/* nodes */}
      {profile.bands.map((b, i) => {
        const y = b.type === "BP" || b.type === "NO" ? dy(0) : dy(b.gain);
        const isSel = i === selected;
        return (
          <g
            key={i}
            onPointerDown={(e) => {
              (e.target as Element).setPointerCapture?.(e.pointerId);
              onSelect(i);
              setDrag(i);
            }}
            onWheel={(e) => {
              const q = Math.max(0.1, Math.min(10, Math.round((b.q * (e.deltaY < 0 ? 1.1 : 0.9)) * 100) / 100));
              update(i, { q });
            }}
            onDoubleClick={() => update(i, { enabled: !b.enabled })}
            style={{ cursor: "grab" }}
          >
            <circle cx={fx(b.freq)} cy={y} r={isSel ? 8 : 6.5} fill={isSel ? "rgb(var(--md-primary))" : "rgb(var(--md-surface-container-highest))"} stroke="rgb(var(--md-primary))" strokeWidth={1.5} opacity={b.enabled ? 1 : 0.35} />
            <text x={fx(b.freq)} y={y + 2.8} fontSize={7.5} fontWeight={700} textAnchor="middle" fill={isSel ? "rgb(var(--md-on-primary))" : "rgb(var(--md-on-surface))"} pointerEvents="none">
              {i + 1}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
