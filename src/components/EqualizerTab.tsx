import { useEffect, useMemo, useRef, useState } from "react";
import { Cpu, Monitor, Plus, Trash2, Upload, Download, CircleCheck, CircleAlert, Wand2 } from "lucide-react";
import { Segmented, Slider, Switch, VSlider } from "./ui";
import PeqGraph from "./PeqGraph";
import { isPending, useEarbudsStore } from "@/store/useEarbudsStore";
import { autoPreamp, flatProfile, fmtFreq } from "@/lib/biquad";
import { ALL_PEQ_PRESETS, presetToProfile, recommendedFor } from "@/presets/autoeqPresets";
import type { DeviceState, FilterType, HostEqProfile } from "@/types/earbuds";

const FILTERS: { v: FilterType; l: string }[] = [
  { v: "PK", l: "Peak" },
  { v: "LSC", l: "Low Shelf" },
  { v: "HSC", l: "High Shelf" },
  { v: "BP", l: "Bandpass" },
  { v: "NO", l: "Notch" },
];

function HardwareEq({ device }: { device: DeviceState }) {
  const send = useEarbudsStore((s) => s.send);
  const caps = device.caps;
  const connected = device.connection === "connected";
  const [gains, setGains] = useState<number[]>(device.eq.customGains);
  useEffect(() => setGains(device.eq.customGains), [device.eq.customGains]);
  const pendingPreset = isPending(device.pending, "eq");
  const pendingCustom = isPending(device.pending, "customEq");
  const customActive = caps.customEqPresetId !== null && device.eq.presetId === caps.customEqPresetId;
  const [lo, hi] = caps.customEqRange;

  return (
    <>
      <section className="card">
        <div className="card-title">
          Пресеты {device.model}
          {pendingPreset && <span className="ml-auto text-[10px] normal-case tracking-normal text-primary pulse-pending">ожидание ACK…</span>}
        </div>
        <div className="flex flex-wrap gap-1.5">
          {caps.eqPresets.map((p) => (
            <button
              key={p.id}
              disabled={!connected}
              className={`chip !text-[11.5px] ${device.eq.presetId === p.id ? "chip-active" : ""}`}
              onClick={() => send({ type: "setEqPreset", id: p.id })}
            >
              {p.name}
            </button>
          ))}
        </div>
      </section>

      {caps.customEqBands.length > 0 && (
        <section className={`card ${pendingCustom ? "pulse-pending" : ""}`}>
          <div className="card-title">
            <span className="flex-1">Свой EQ · {caps.customEqBands.length} полос · в наушниках</span>
            <span className={`rounded-full px-2 py-0.5 text-[9.5px] normal-case tracking-normal ${customActive ? "bg-primary text-on-primary" : "bg-surface-c-highest"}`}>
              {customActive ? "активен" : "не выбран"}
            </span>
          </div>
          <div className="flex justify-between px-0.5">
            {caps.customEqBands.map((f, i) => (
              <div key={f} className="flex flex-col items-center gap-1">
                <span className="font-mono text-[9.5px] text-on-surface">
                  {(gains[i] ?? 0) > 0 ? "+" : ""}
                  {gains[i] ?? 0}
                </span>
                <VSlider
                  min={lo}
                  max={hi}
                  step={1}
                  value={gains[i] ?? 0}
                  disabled={!connected}
                  onChange={(v) => setGains((g) => g.map((x, j) => (j === i ? v : x)))}
                  onCommit={(v) => send({ type: "setCustomEq", gains: gains.map((x, j) => (j === i ? v : x)) })}
                />
                <span className="text-[9.5px] text-on-surface-variant">{fmtFreq(f)}</span>
              </div>
            ))}
          </div>
          <div className="mt-2 flex justify-end">
            <button className="btn-outline !py-1" disabled={!connected} onClick={() => send({ type: "setCustomEq", gains: gains.map(() => 0) })}>
              Сбросить
            </button>
          </div>
        </section>
      )}

    </>
  );
}

function HostEq({ device }: { device: DeviceState }) {
  const snap = useEarbudsStore((s) => s.snap)!;
  const setHostEq = useEarbudsStore((s) => s.setHostEq);
  const installHostEq = useEarbudsStore((s) => s.installHostEq);
  const backend = useEarbudsStore((s) => s.backend);
  const [profile, setProfile] = useState<HostEqProfile>(snap.hostEq);
  const [sel, setSel] = useState(0);
  const timer = useRef<number | null>(null);
  const latest = useRef(profile);
  const fileRef = useRef<HTMLInputElement>(null);
  const status = snap.hostEqStatus;
  const rec = useMemo(() => recommendedFor(device.modelCode), [device.modelCode]);

  const commit = (p: HostEqProfile) => {
    const withPre = p.autoPreamp ? { ...p, preamp: autoPreamp(p) } : p;
    setProfile(withPre);
    latest.current = withPre;
    if (timer.current === null) {
      timer.current = window.setTimeout(() => {
        timer.current = null;
        setHostEq(latest.current);
      }, 40);
    }
  };

  const band = profile.bands[sel];
  const upd = (patch: Partial<HostEqProfile["bands"][number]>) =>
    commit({ ...profile, bands: profile.bands.map((b, i) => (i === sel ? { ...b, ...patch } : b)) });

  return (
    <>
      <section className="card">
        <div className="mb-1 flex items-center gap-2">
          <span className="card-title !mb-0 flex-1">Студийный параметрический EQ</span>
          <Switch checked={profile.enabled} onChange={(v) => commit({ ...profile, enabled: v })} />
        </div>
        <div className={`mb-2 flex items-start gap-1.5 rounded-xl px-2 py-1.5 text-[10.5px] ${status.active ? "bg-success/15 text-success" : status.available ? "bg-surface-c-high text-on-surface-variant" : "bg-warning/15 text-warning"}`}>
          {status.active ? <CircleCheck size={13} className="mt-px shrink-0" /> : <CircleAlert size={13} className="mt-px shrink-0" />}
          <span className="flex-1">
            <b>{status.backend === "pipewire" ? "PipeWire" : status.backend === "equalizerApo" ? "Equalizer APO" : "Нет аудио-бэкенда"}</b> — {status.message}
          </span>
          {status.available && !status.active && profile.enabled && (
            <button className="shrink-0 underline" onClick={() => installHostEq()}>
              Установить
            </button>
          )}
        </div>
        <PeqGraph profile={profile} selected={sel} onSelect={setSel} onChange={commit} />
        <div className="mt-1 text-center text-[9.5px] text-on-surface-variant">Тяните точки · колесо = Q · двойной клик = вкл/выкл полосы</div>
      </section>

      {band && (
        <section className="card">
          <div className="mb-2 flex items-center gap-2">
            <span className="card-title !mb-0">Полоса {sel + 1}</span>
            <div className="ml-auto flex gap-1">
              {profile.bands.map((_, i) => (
                <button key={i} onClick={() => setSel(i)} className={`h-5 w-5 rounded-full text-[9.5px] font-bold ${i === sel ? "bg-primary text-on-primary" : "bg-surface-c-highest text-on-surface-variant"}`}>
                  {i + 1}
                </button>
              ))}
            </div>
          </div>
          <div className="grid grid-cols-[1fr_auto] items-center gap-x-2 gap-y-1.5">
            <select className="select" value={band.type} onChange={(e) => upd({ type: e.target.value as FilterType })}>
              {FILTERS.map((f) => (
                <option key={f.v} value={f.v}>
                  {f.l} ({f.v})
                </option>
              ))}
            </select>
            <Switch checked={band.enabled} onChange={(v) => upd({ enabled: v })} />
          </div>
          <div className="mt-2 grid grid-cols-3 gap-2 text-[10px] text-on-surface-variant">
            <label className="flex flex-col gap-0.5">
              Fc, Гц
              <input className="num" type="number" min={20} max={20000} value={band.freq} onChange={(e) => upd({ freq: Math.max(20, Math.min(20000, Number(e.target.value) || 20)) })} />
            </label>
            <label className="flex flex-col gap-0.5">
              Gain, дБ
              <input className="num" type="number" step={0.1} min={-20} max={20} value={band.gain} disabled={band.type === "BP" || band.type === "NO"} onChange={(e) => upd({ gain: Math.max(-20, Math.min(20, Number(e.target.value) || 0)) })} />
            </label>
            <label className="flex flex-col gap-0.5">
              Q
              <input className="num" type="number" step={0.05} min={0.1} max={10} value={band.q} onChange={(e) => upd({ q: Math.max(0.1, Math.min(10, Number(e.target.value) || 0.7)) })} />
            </label>
          </div>
          <div className="mt-2 flex gap-1.5">
            <button className="btn-outline !py-1" disabled={profile.bands.length >= 10} onClick={() => { commit({ ...profile, bands: [...profile.bands, { enabled: true, type: "PK", freq: 1000, gain: 0, q: 1.41 }] }); setSel(profile.bands.length); }}>
              <Plus size={12} /> Полоса
            </button>
            <button className="btn-outline !py-1" disabled={profile.bands.length <= 1} onClick={() => { commit({ ...profile, bands: profile.bands.filter((_, i) => i !== sel) }); setSel(Math.max(0, sel - 1)); }}>
              <Trash2 size={12} /> Удалить
            </button>
          </div>
        </section>
      )}

      <section className="card">
        <div className="card-title">Preamp (защита от клиппинга)</div>
        <div className="flex items-center gap-3">
          <Slider min={-24} max={6} step={0.1} value={profile.preamp} disabled={profile.autoPreamp} onChange={(v) => commit({ ...profile, preamp: v })} />
          <span className="w-12 text-right font-mono text-xs">{profile.preamp.toFixed(1)} dB</span>
        </div>
        <label className="mt-1 flex items-center gap-2 text-[11px] text-on-surface-variant">
          <Switch checked={profile.autoPreamp} onChange={(v) => commit({ ...profile, autoPreamp: v })} /> Авто (−макс. усиление кривой)
        </label>
      </section>

      <section className="card">
        <div className="card-title">AutoEq / пресеты</div>
        {rec && (
          <button className="btn-tonal mb-2 w-full" onClick={() => { commit({ ...presetToProfile(rec, true) }); setSel(0); }}>
            <Wand2 size={13} /> Рекомендовано: {rec.name}
          </button>
        )}
        <select
          className="select"
          value=""
          onChange={(e) => {
            const p = ALL_PEQ_PRESETS.find((x) => x.id === e.target.value);
            if (p) {
              commit(p.bands.length ? presetToProfile(p, profile.enabled) : { ...flatProfile(), enabled: profile.enabled });
              setSel(0);
            }
          }}
        >
          <option value="" disabled>
            Загрузить пресет…
          </option>
          {ALL_PEQ_PRESETS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name} — {p.source}
            </option>
          ))}
        </select>
        <div className="mt-2 flex gap-1.5">
          <button className="btn-outline flex-1 !py-1" onClick={() => fileRef.current?.click()}>
            <Upload size={12} /> Импорт APO .txt
          </button>
          <button
            className="btn-outline flex-1 !py-1"
            onClick={async () => {
              const text = (await backend?.exportPeqText(profile)) ?? "";
              const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
              const a = document.createElement("a");
              a.href = url;
              a.download = `${profile.name.replace(/[^\w\-]+/g, "_") || "budspulse"}_ParametricEQ.txt`;
              a.click();
              URL.revokeObjectURL(url);
            }}
          >
            <Download size={12} /> Экспорт
          </button>
          <input
            ref={fileRef}
            type="file"
            accept=".txt"
            hidden
            onChange={async (e) => {
              const f = e.target.files?.[0];
              if (!f || !backend) return;
              const parsed = await backend.parsePeqText(await f.text());
              commit({ ...parsed, name: f.name.replace(/\.txt$/i, ""), enabled: profile.enabled });
              setSel(0);
              e.target.value = "";
            }}
          />
        </div>
      </section>
    </>
  );
}

export default function EqualizerTab({ device }: { device: DeviceState }) {
  const mode = useEarbudsStore((s) => s.snap?.settings.eqMode ?? "hardware");
  const updateSettings = useEarbudsStore((s) => s.updateSettings);
  if (device.connection !== "connected") {
    // the PC-side EQ works without the control channel
    return (
      <>
        <div className="rounded-2xl bg-surface-c-high px-3 py-2 text-[10.5px] leading-snug text-on-surface-variant">
          Наушники не подключены — эквалайзер в наушниках недоступен. Студийный EQ на ПК работает всегда.
        </div>
        <HostEq device={device} />
      </>
    );
  }
  return (
    <>
      <Segmented
        layoutId="eq-mode"
        value={mode}
        onChange={(v) => updateSettings({ eqMode: v })}
        options={[
          { value: "hardware", label: "В наушниках", icon: <Cpu size={13} /> },
          { value: "host", label: "Студийный ПК", icon: <Monitor size={13} /> },
        ]}
      />
      {mode === "hardware" ? <HardwareEq device={device} /> : <HostEq device={device} />}
    </>
  );
}
