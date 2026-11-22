# Nothing / CMF by Nothing — `0x55` RFCOMM control protocol (byte-level spec)

Target devices (priority): **Nothing Ear (2024) = model `B171`**, **CMF Buds Pro 2 = `B172` (rev. `B187`, newer rev. `B195`)**.
Everything below was taken from real source code (pinned commits, see §0) or from real captures
in Bharadwaj Raju's blog. Each fact has a reference tag like **[EW:bs:407]** = ear-web
`res/js/bluetooth_socket.js` line 407. Anything not backed by at least one source is marked
**[UNVERIFIED]**. Where sources disagree, both readings are shown as **[CONFLICT]**.

All 16-bit IDs are written as numbers (`0xF00F`); on the wire they are **little-endian** (`0F F0`).
Byte offsets count from 0 at the `0x55` SOF byte ("frame offset") unless "payload offset" is stated
(payload offset 0 = frame offset 8).

---

## 0. Sources (pinned)

| Tag | Source | Revision read |
|---|---|---|
| EW | github.com/radiance-project/ear-web — the original reverse-engineered web app. Files: `res/js/bluetooth_socket.js` (bs), `device_common.js` (dc), `gestures_common.js` (gc), `control.js` (ctl), `nothing_connected.js` (nc), `eq_two.js`, `eq_espeon.js`, `twos.js` (B171 page), `espeon.js` (B172/B195 page), `audio_codec.js`, `ear_config_file.json` (cfg), `MainControl/MainControl_twos.html`, `MainControl_espeon.html` | `89f69fbab8550d6b5ba8cd0c47e704a1c057ac1d` (2026-10-31) |
| BL | github.com/maniacx/BudsLink — `src/lib/devices/nothingBuds/nothingBudsSocket.js` (sock), `nothingBudsConfig.js` (conf), `deviceConfigs/NothingEar.js` (B171), `CMFBudsPro2.js` (B172), `CMFBudspro2.js` (B187) | `f4f98c0ef21ecaa108ab1685834daabc590e1424` (2026-11-13) |
| GB | Gadgetbridge (codeberg.org/Freeyourgadget/Gadgetbridge) — `service/devices/nothing/Ear1Support.java` (inner class `NothingProtocol`), `devices/nothing/CmfBudsPro2Coordinator.java`, `devices/nothing/prefs/NothingAudioMode.java`, `NothingEqualizer.java`, `NothingTapAction.java`, `util/CheckSums.java` | codeberg `3c30da08b93ac25a90812e6a84a5df1de6527708` (2026-11-14); older GitHub mirror `a0948ee1` for `NothingProtocol.java` |
| NC | github.com/FormalSnake/nothingctl — `docs/protocol.md` (clean-room transcription of ear-web, with per-model golden frames), `src/protocol/parser.rs` | `b89088ec61ae1a77a2e75e365b4ff1c63057c611` |
| ET | github.com/Vadim-Karpenko/ear-tray — `internal/protocol/frame.go`, `internal/bt/winrt_windows.go`, `internal/device/models.go` | `32a1b8ec2aa8dbec4d2aeae47e00fd05274c62c3` |
| SX | github.com/SoaOaoS/something-x — `nothing_app/protocol.py` (comments claim APK-decompilation origin) | `b9fa63eba739f56064bce7e4ab664279795ae17f` |
| ND | github.com/ezvk/nadamas — `nadamas/protocol.py`, `nadamas/models.py`, `nadamas/models/cpb2.json` (CMF Buds Pro 2, "verified": true, read from a real device) | `4b3b3d950f3aa5caac57517864d5981e5dc9a939` |
| EA | github.com/oneydef/EarA-linux — `eara/models.py`, `LINEUP.md` | `e33dcef4aad69f68fb77b95f60ae55166a294c61` |
| BR | Bharadwaj Raju, "Creating a Linux controller for the Nothing Ear (2)", https://bharadwajraju.com/posts/nothing-ear-2-on-linux/ — **real HCI captures** from a Nothing Ear (2) (B155) | 2024-01-06 |

URL forms: `https://github.com/radiance-project/ear-web/blob/89f69fb/res/js/bluetooth_socket.js#L407`,
`https://codeberg.org/Freeyourgadget/Gadgetbridge/src/commit/3c30da08/app/src/main/java/nodomain/freeyourgadget/gadgetbridge/service/devices/nothing/Ear1Support.java`, etc.

Self-check performed: an independent Python encoder (CRC below) reproduces byte-for-byte (a) every
captured host frame in [BR] (`…cb 01 07 00 c5 af`, `…1e c0 01 00 0c 03 98 19`, `…18 c0 00 00 51 39 21`,
`…42 c0 00 00 03 e0 d1`, `…06 c0 00 00 05 90 dc`, `…14 f0 01 00 2a 01 43 16`, `…03 f0 05 00 59 01 02 01 03 09 de 71`),
(b) the CRCs on **device→host** replies captured in [BR] (firmware reply `fa 36`, ANC reply `10 7a`, gesture
reply `e0 30`, serial reply `91 d1`), and (c) nothingctl's golden frames (ANC, listening mode, custom EQ 3400 Hz and 6900 Hz, bass).

---

## 1. Corrections vs. the draft (`claude_code_earbuds_widget_prompt.md` §2.1)

| # | Draft says | Reality (source) |
|---|---|---|
| 1 | ANC golden frames High/Mid/Low/Adaptive with CRCs `e6 6f`, `e6 9f`, `e7 0f`, `e5 3f` | The **mode bytes are right** (01 High, 02 Mid, 03 Low, 04 Adaptive, 05 Off, 07 Transparency — [EW:bs:361-424], [EW:dc:689-731], [BL:B171], [GB NothingAudioMode], [SX], [ND]). The CRCs were copied with typos from [BR]'s table. Correct frames (same FSNs): High `55 60 01 0f f0 03 00 cf 01 01 00 c7 3f`, Mid `… d5 01 02 00 c0 d7`, Low `… d7 01 03 00 c0 ff`, Adaptive `… dd 01 04 00 c1 17`. |
| 2 | Battery notification `0xE007` (`07 E0`) | Battery push is **`0xE001`** (`01 E0`); read reply is `0x4007` ([EW:bs:179], [GB], [BL:conf], [SX], [ET]). |
| 3 | Nothing EQ presets `0..3`, **4 = Custom, 5 = Advanced, 6 = Dirac** | Nothing Ear (B171) uses `0xF010`/`0xC01F`: 0 Balanced, 1 Voice, 2 More Treble, 3 More Bass, **5 Custom**, **6 = Advanced (only as a read value; set via `0xF04F`)**. There is no "4" and no Dirac on B171 ([EW eq_two.js + MainControl_twos.html], [BL:B171]). Dirac Opteo belongs to CMF listening modes (§6.2). |
| 4 | CMF presets as names only, same command | CMF Buds Pro 2 uses a **different command pair `0xF01D`/`0xC050`** ("listening mode"): 0 Dirac Opteo, 1 Rock, 2 Electronic, 3 Pop, 4 Enhance Vocals, 5 Classical, 6 Custom ([EW MainControl_espeon.html], [BL:B172/B187]). [CONFLICT] Gadgetbridge puts Dirac at `0x07`. |
| 5 | Custom EQ = `[bass:i8, mid:i8, treble:i8]` | Custom EQ is a **53-byte parametric packet of little-endian float32s** via `0xF041` (§6.3). Band frequencies differ per model (B171 high-shelf 3400 Hz; B172/B187 6900 Hz). |
| 6 | Ultra Bass: no command given | `0xC04E` read / `0xF051` write, payload `[enabled, level*2]` (levels 1–5 → 2,4,6,8,10). B195 uses 3 stages (0,5,10) ([EW:bs:528-563]). |
| 7 | Gesture offsets 12/20/28/36… | Those are frame offsets of one Ear (2) capture; **parse the record list** instead: `[count][dev,button,type,action]×count` ([EW:bs:1214-1232], [BR], [BL:sock:859-884]). Gesture types: 1 single, 2 double, 3 triple, 7 hold, 9 double-hold, 10 rotate (case knob). |
| 8 | Fit test result values 0/1 | `0xE00D` push, payload `[left, right]`: 0 good, 1 adjust, **2 not detected** ([BL:sock:806-830], [NC]). |
| 9 | Draft frame layout | Correct, but: CTRL bit `0x20` = **CRC present**; device **pushes (`55 00 01 …`) carry no CRC**; LEN is u16 LE; responses **echo the request FSN**. Stream splitting must use `8 + LEN + (CRC? 2 : 0)` (§2). |
| 10 | "firmware silently drops ALL SETs if one frame lacked CRC" | Only a comment in [SX] (claims APK origin). **[UNVERIFIED]**. Just always send CRC; ear-web/BudsLink/ear-tray always do. |
| 11 | "Fast Pair Service UUID (read-only Model ID)" | `df21fe2c-2515-4fdb-8886-f12c4d67927c` is a separate **RFCOMM** service (Google Fast Pair Message Stream). On open, the device sends a 7-byte message whose bytes 4..6 are the 3-byte Fast Pair model ID ([EW:nc:101-156]). Not needed if you use `0xC01C`. |
| 12 | Model list | Mostly right. Add: `B187` & `B195` (CMF Buds Pro 2 revisions), `B201` (Ear (3) rev.), `B190` Ear (3a), `B189` CMF Clip Pro, `B192` Headphone (1) Pro, `B193` CMF Buds Neo, `B197` CMF Buds 2 rev. ([EW:ctl], [BL]). `0xC01C` reply is the model code little-endian (`71 B1` → B171). |
| 13 | Channel 15 fallback | Ear (2) used channel 15 [BR]; CMF Buds 2 uses **16** [EA LINEUP.md]. Resolve by SDP on UUID `aeac4a03-…` ([EW:bs:116], [GB], [ET]). |
| 14 | Hi-Res selector 0/1/2 on `0xC029`/`0xF01C` | Correct ([EW:bs:1385-1422]), but **CMF Buds Pro 2 has no LHDC** (cfg `highQualityAudio: 4` = LDAC only); Ear (2024) has `7` (LHDC+LDAC). Changing it reboots/reconnects the buds. |
| 15 | Low-lag frames | Correct. Read is `0xC041` → `0x4041`, payload[0]: 1 = on, 2 = off ([EW:bs:1144-1169]). |

---

## 2. Framing

### 2.1 Layout (both directions)

| Frame offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | SOF = `0x55` | [EW:bs:33], [GB], [BL:sock:14], [SX] |
| 1..2 | 2 | CTRL, u16 LE | Host always sends `60 01` = `0x0160` ([EW:bs:33], [BL], [ET], [SX]) |
| 3..4 | 2 | CMD, u16 LE | e.g. `0F F0` = `0xF00F` |
| 5..6 | 2 | LEN = payload length, u16 LE | ear-web writes only byte 5 and leaves byte 6 = 0 [EW:bs:39-40]; GB/BL/SX treat it as u16 LE ([GB `putShort(payload.length)`], [BL:sock:149]). Serial reply has LEN `0x0088` [BR]. |
| 7 | 1 | FSN / operation id | see §2.4 |
| 8 .. 8+LEN-1 | LEN | payload | |
| 8+LEN .. 9+LEN | 2 | CRC-16, LE (low byte first) | **present iff CTRL & 0x0020** ([GB `isCrcNeeded`], [BL:sock:148], [SX:625]) |

No escaping/byte-stuffing ([NC §2]).

### 2.2 CTRL bits (from Gadgetbridge constants)

| Mask | Meaning | Source |
|---|---|---|
| `0x0020` | CRC appended | [GB `CONTROL_CRC = 0x20`] |
| `0x0040` | set when the host uses an incrementing FSN ("incrementCounter") | [GB `control | 0x40` when incrementCounter] |
| `0x0F00` | device type; `1` = TWS headset (value `0x01xx`) | [GB `MASK_DEVICE_TYPE`, `CONTROL_DEVICE_TYPE_TWS_HEADSET = 1`] |
| `0x001F` | response/status code; `0` = OK (GB drops frames with nonzero) | [GB `MASK_RSP_CODE`, `isOk`] |

So host frames are `0x0160` (TWS + FSN + CRC). Gadgetbridge sends `0x0120` (no `0x40`, FSN fixed 0) to
**CMF Buds Pro 2** (`incrementCounter() = false`) and it works there [GB CmfBudsPro2Coordinator].
Observed device frames: replies `55 60 01 …` (CRC present, e.g. firmware reply ends `fa 36`), pushes
`55 00 01 …` (CTRL `0x0100`, **no CRC**, e.g. fit-test push `55 00 01 0d e0 02 00 00 01 00`, 10 bytes) [BR].

### 2.3 CRC

CRC-16/MODBUS: init `0xFFFF`, reflected poly `0xA001`, no final XOR, computed over **frame offset 0 (SOF)
through the last payload byte**, appended **low byte first** ([EW:bs:65-74], [GB CheckSums.getCRC16ansi],
[BL table-driven crc16Ansi], [ET CRC16], [SX _crc16]). Check value for ASCII "123456789" is `0x4B37` [NC §2].

```python
def crc(d):
    c = 0xFFFF
    for b in d:
        c ^= b
        for _ in range(8):
            c = (c >> 1) ^ 0xA001 if c & 1 else c >> 1
    return c
```

### 2.4 FSN (byte 7)

* ear-web: global counter, incremented before every send, reset to 1 once ≥ 250 [EW:bs:34-35, 282-285].
* BudsLink: `seq = seq >= 250 ? 1 : seq + 1` [BL:sock:105].
* Gadgetbridge: 0..0xFC wrap when incrementing; constant 0 for CMF Buds Pro 2 [GB].
* The device **echoes the request FSN in its reply**: req FSN `03` → firmware reply byte7 `03`; `0c` → `0c`;
  `51` → `51`; `05` → `05` [BR captures]. BudsLink uses this to match request/response with a 100 ms timeout
  [BL:sock:60-102]. ear-web ignores it and dispatches purely by CMD [EW:bs:176-277].
* Recommendation: 1..250 rolling, one outstanding request at a time (BudsLink style), dispatch by CMD.

### 2.5 Command-ID classes (observed pattern)

| High nibble | Meaning | Example | Source |
|---|---|---|---|
| `0xC` | host GET request | `0xC007` | all |
| `0xF` | host SET request | `0xF00F` | all |
| `0x4` | device reply to GET (= request with bit 15 cleared) | `0x4007` | [EW:bs:179-277], [GB `command | 0x8000`], [SX:636] |
| `0x7` | device ACK to SET (bit 15 cleared) | `0x7004` / `0x700F` (`55 20 01 04 70 00 00 00`), `0x7040`, `0x7051` | [GB Ear1Support:148-159 comments], [BL:conf `LATENCY_NTFY 0x7040`, `ENHANCED_BASS_NTFY 0x7051`] |
| `0xE` | device-initiated event / push | `0xE001` battery, `0xE002` wear/case, `0xE003` ANC, `0xE00D` fit test, `0xE00E` dual-connect device | [EW], [GB], [BL], [SX] |
| `0x6` | alternate push form seen by ear-web (handled like `0xE0xx`) | `0x6003` (ANC), `0x6019`, `0x601C`, `0x6029` | [EW:bs:182, 251-258] |

Normalising rule used by GB/SX: `req_id = rx_cmd | 0x8000` (maps `0x40xx→0xC0xx`, `0x70xx→0xF0xx`; `0xE0xx` unchanged).

### 2.6 Splitting a byte stream into frames (recommended algorithm)

ear-web assumes one frame per read() chunk and ignores CRC — **do not copy that** [NC §2].
Use the BudsLink / something-x approach ([BL:sock:137-164], [SX:620-639]):

1. Append incoming bytes to a buffer.
2. Discard bytes until `buf[0] == 0x55`.
3. Need ≥ 8 bytes. `ctrl = u16le(buf[1..3])`, `len = u16le(buf[5..7])`, `total = 8 + len + (ctrl & 0x20 ? 2 : 0)`.
4. Wait until `buf.len() >= total`. If CRC present, verify; on mismatch drop 1 byte and resync
   (ear-tray drops; something-x only logs) [ET Parse], [SX:630-634].
5. Emit `(ctrl, cmd=u16le(buf[3..5]), fsn=buf[7], payload=buf[8..8+len])`, consume `total`, loop.
6. Cap the buffer (BudsLink clears at 1500 bytes; nothingctl 4096).

---

## 3. Transport: RFCOMM service

* Control service class UUID **`aeac4a03-dff5-498f-843a-34487cf133eb`** ([EW:bs:116], [GB `UUID_DEVICE_CTRL`],
  [ET winrt_windows.go:39], [EA]). Resolve channel via SDP; do not hard-code.
  * Observed channels: 15 on Ear (2) [BR]; 16 on CMF Buds 2 [EA LINEUP.md]. something-x probe order when SDP
    fails: 15, 17, 16, 18, 14 … and it validates by sending a `0xC001` probe and requiring a `0x55` reply
    (other channels answer with AT strings / `0x03` legacy headers) [SX:96, 507-565].
* Windows: ear-tray opens it through WinRT `RfcommDeviceService` by that UUID (device id string
  `Bluetooth#Bluetooth<local>-<remote>#RFCOMM:00000000:{aeac4a03-…}`) [ET winrt_windows.go:38-80].
* Fast Pair UUID `df21fe2c-2515-4fdb-8886-f12c4d67927c`: separate RFCOMM service; ear-web opens it, sends
  nothing, waits for a 7-byte chunk and uses bytes 4..6 as the Fast Pair model ID, then closes it [EW:nc:101-156].
  (Byte 0..3 = Fast Pair Message Stream header `03 01 00 03` — group "device information", code "model id",
  length 3 **[UNVERIFIED]**, from the Fast Pair spec, ear-web doesn't check them.)
* Single client / busy socket: something-x retries once after 3 s on `EBUSY` ("stale connection") [SX:517-521].
  Whether the phone's Nothing X and a PC can hold the control channel at the same time: **[UNVERIFIED]**.
* No baud rate meaning (ear-web's 9600 is a Web-Serial API requirement) [NC §1].

---

## 4. Session start & model identification

### 4.1 Connect sequences used by real clients

* **ear-web** (no handshake): `0xC007` battery, `0xC01F` EQ (Nothing) **or** `0xC050` listening mode (CMF),
  `0xC042` firmware, `0xF00A` UTC time, `0xC00E` in-ear, `0xC041` latency, (`0xC020` B155 only), `0xC018`
  gestures, `0xC01E` ANC, `0xC04C` advanced-EQ status, `0xC04E` bass — 100 ms apart, no waiting for replies
  [EW:bs:76-102]. After firmware reply it sends config-driven reads `0xC027`, `0xC028 00`, `0xC029`,
  `0xC04F`, `0xC022`/`0xC05A` … [EW:bs:1031-1040], [NC §3].
* **BudsLink**: `0xC001` (get protocol), `0xF001` (set protocol), `0xC01C` (model) → on model known:
  battery, EQ, firmware, serial, UTC, in-ear, latency, personalized ANC, gestures, `0xC01D`+`0xC01E` ANC, bass,
  spatial, dual … one request in flight at a time [BL:sock:275-333].
* **something-x / nadamas**: `0xC001` probe → on reply send `0xF001` ("activation", APK `isNeedActivate`),
  then `0xC007`, `0xC01E [03]`, `0xC00A`, `0xC042`, `0xC006` (+ nadamas `0xC01C`); 3 s fallback if no
  `0x7001` ACK [SX:643-663, 999-1009].
* Recommendation: send `0xC001`, `0xF001` (harmless; optional), `0xC01C`, then the reads. Never assume a
  reply arrives for unsupported commands: CMF Buds Pro 2 answers **every** query, unsupported ones with an
  **empty payload** [ND cpb2.json notes].

### 4.2 Model code — `0xC01C` (preferred)

* Request `0xC01C`, no payload. Reply `0x401C`, payload `[lo, hi]`; model = `"B" + hex(hi<<8|lo)` … i.e.
  `71 B1` → `0xB171` → "B171"; `87 B1` → "B187" (read from a real CMF Buds Pro 2, fw 1.0.1.74) [BL:sock:358-377],
  [ND models.py doc, cpb2.json `"model_codes": ["87b1"]`].
* B171 = Nothing Ear (2024, product name "Nothing Ear"); B172 = CMF Buds Pro 2; B187 = CMF Buds Pro 2 (2nd
  hw rev., config `supportId: "B172"`); B195 = CMF Buds Pro 2 (newer rev., `supportId: "B172"`)
  [EW:cfg], [EW:ctl FASTPAIR_MODELS], [BL deviceConfigs].
* Fallback: Bluetooth name. BudsLink regexes: B171 `/^.*Nothing Ear$/`, B172/B187 `/^.*CMF Buds Pro 2$/` [BL];
  Gadgetbridge `^CMF Buds Pro 2$` [GB].

### 4.3 Firmware — `0xC042`

Reply `0x4042`, payload = ASCII version string, LEN = string length (e.g. `1.0.1.101`, LEN 9) [BR], [EW:bs:964-973].
ear-web uses firmware to pick per-firmware config ranges [EW:bs:975-1108].

### 4.4 Serial / "remote configuration" — `0xC006`

Reply `0x4006`. Payload byte 0 = a count/flags byte (`0x09` in [BR]; BudsLink skips it [BL:sock:421]), then
UTF-8 text lines `device,field,value\n`. Device 2 = left, 3 = right, 4 = case; field 1 = (hw/proto?) version,
field 2 = firmware, **field 4 = serial** (e.g. `SH10182307002807`) [BR capture], [BL:sock:415-437], [SX:683-696].

**SKU from serial** (used by something-x and EarA): for serials starting `SH` or `13`, SKU = `serial[4:6]`
[SX:236-253], [EA models.py:153-184]. Check: [BR]'s Ear (2) serial `SH10182307002807` → `18` → B155 ✓.

| Base | SKU digits (SX) | SKU digits (EA) |
|---|---|---|
| B171 Nothing Ear (2024) | 61, 62, 69, 70, 74, 75 | 61, 62, 69, 70, **71**, 74, 75 |
| B172 CMF Buds Pro 2 | 76–83 | 76–83 |
| B162 Ear (a) | 63–68, **71**, 72, 73 | 63–68, 72, 73 |
| B155 Ear (2) | 17, 18, 19, 27, 28, 29 | same |
| B181 Ear (1) | 01–04, 06–08, 10 | same |
| B157 Ear (stick) | 14–16 (or `MA` serials yr 22/23) | 14–16 |
| B163 / B164 / B168 | 30–35 / 48–53 / 54–59 | same |

[CONFLICT] SKU `71`: B162 per [SX], B171 per [EA]. Prefer `0xC01C`. SKU tables for B187/B195: none found.

### 4.5 Fast Pair model IDs (24-bit, from the Fast Pair RFCOMM channel) [EW:ctl]

* B171 Nothing Ear: `FEB1C7` (white), `A20444` (black).
* B172 CMF Buds Pro 2: `F29566`, `CA36A6`, `A7B220`, `2B353E`; B187 rev: `2F45F5`, `E1BE45`, `1253C0`, `0F1A4F`;
  B195 rev: `F51D3D`, `A544CE`, `9EAC04`, `1D07D4`, `53BFE6`, `072714`.
* Full table (all models) is in `res/js/control.js` `FASTPAIR_MODELS`.

### 4.6 Model code ↔ name map

B181 Ear (1) · B157 Ear (stick) · B155 Ear (2) · B162/B183 Ear (a) · **B171 Nothing Ear (2024)** · B173/B201 Ear (3) ·
B174 Ear (open) · B190 Ear (3a) · B170 Headphone (1) · B192 Headphone (1) Pro · B186/B198 Headphone (a) ·
B163 CMF Buds Pro · B164 CMF Neckband Pro · B168 CMF Buds · **B172/B187/B195 CMF Buds Pro 2** · B179/B197 CMF Buds 2 ·
B184 CMF Buds 2 Plus · B185 CMF Buds 2a · B175 CMF Headphone Pro · B189 CMF Clip Pro · B193 CMF Buds Neo
([EW:ctl], [EW:nc:1-50], [BL deviceConfigs]).

### 4.7 UTC time — `0xF00A`

Payload = 4 bytes, Unix seconds **big-endian**; ear-web sends it on every connect except B181 [EW:bs:1195-1208], [BL:sock:439-449].

---

## 5. Battery

* GET `0xC007`, no payload → reply `0x4007`; unsolicited push **`0xE001`**; same payload format
  [EW:bs:179-181, 314-351], [GB], [BL:conf], [SX:66].
* Payload: `[N]` then N × `[device_id, level]`.
  * device_id: `0x02` left, `0x03` right, `0x04` case, `0x06` single/stereo (headphones).
  * level: bits 0–6 = percent (`& 0x7F`), bit 7 = charging (`& 0x80`).
  [EW:bs:317-336], [GB handleBatteryInfo], [BL:sock:456-500], [SX:735-774].
* Entries for absent units are omitted (count shrinks) — e.g. case missing when lid closed **[UNVERIFIED]**, inferred
  from the count-prefixed format; nothingctl's test vector `55 60 01 07 40 03 00 01 01 06 8d` is synthetic [NC parser.rs].
* `0xE002` push is sent (twice) when the case closes with buds inside [GB comment `unk_close_case`]; something-x
  treats `0xE002` as a wear-status trigger and re-reads `0xC00A` [SX:670-676].

### 5.1 Wear status — `0xC00A` (something-x/nadamas only)

Reply payload `[N]` + N × `[type, val]`; type 2 left, 3 right, 4 case, 5 tws, 6 stereo; val bit0 = in case,
bit2 = in ear, bit7 = connected [SX:818-856] (comment cites APK `EarphoneStatus.java`). Only one source → treat
as **[UNVERIFIED by a second source]**.

---

## 6. ANC / noise control

* GET `0xC01E` — no payload [EW:bs:400-405] (or payload `03` [BR], [SX]). Reply `0x401E`; push `0xE003`
  (also `0x6003`) [EW:bs:182-184, 221-223], [BR], [GB], [BL].
* Reply payload = triplets `[type, value, 0x00]`: type 1 = current mode, type 2 = last NC level
  [SX:776-816]. Capture from Ear (2) in Transparency: payload `01 07 00 02 04 00` [BR]. **Mode = payload[1]
  (frame byte 9)** [EW:bs:364], [BL:sock:542].
* SET `0xF00F`, payload `[0x01, mode, 0x00]` [EW:bs:407-424], [GB encodeAudioMode], [BL:sock:550-554], [BR].
  ACK `0x700F` (`55 20 01 0F 70 00 00 00` with GB's 0x0120 ctrl) [GB comment].

| mode byte | Meaning | Sources |
|---|---|---|
| `0x01` | NC High | EW, BL, GB, SX, ND, BR |
| `0x02` | NC Mid | EW, BL, GB, ND, BR |
| `0x03` | NC Low ("light") | EW, BL, GB, SX, ND, BR |
| `0x04` | NC Adaptive | EW, BL, GB, ND, BR |
| `0x05` | Off | all |
| `0x07` | Transparency | all |

Per-model:
* **B171**: config `ancLevel: 63` → transparency (0x10), strength (0x04), mid (0x02), adaptive (0x08) bits → all six [EW:cfg], [EW:dc:5-23], [BL:B171].
* **B172/B187/B195**: config `ancLevel: 63` → all six [EW:cfg]; BudsLink lists all six [BL:B172,B187].
  [CONFLICT] Gadgetbridge offers only ANC(01)/ANCLIGHT(03)/Transparency/Off for CMF Buds Pro 2 [GB CmfBudsPro2Coordinator];
  nadamas (real B187) lists `anc_levels: [1,2,3]` (no Adaptive) [ND cpb2.json]. → Offer Adaptive on B172 but verify
  by reading back `0xC01E` after setting.

---

## 7. EQ

### 7.1 Nothing Ear (2024) B171 — "EQ preset"

* GET `0xC01F` → reply `0x401F`, payload[0] = preset [EW:bs:440-458], [BL:conf].
* SET `0xF010`, payload `[preset, 0x00]` [EW:bs:460-464], [BL:sock:605-621].
* Presets: `0` Balanced, `1` Voice, `2` More Treble, `3` More Bass, `5` Custom; `6` = Advanced (read value only)
  [EW eq_two.js:52-82 + MainControl_twos.html:165-190], [BL:B171 eqPreset].
* [CONFLICT] something-x / nadamas use `{0 Balanced, 1 More Bass, 2 More Treble, 3 Voice}` and a 1-byte payload
  [SX:109-115, 402], [ND]. ear-web + BudsLink (two independent, model-specific sources) agree on 1 = Voice, 3 = More Bass — use those.
* Advanced EQ (B171 `advancedEq: 1`):
  * GET enabled `0xC04C` → `0x404C` payload[0] = 1 enabled; on B171 a 1 means "show Advanced" [EW:bs:426-438].
  * SET enabled `0xF04F`, `[01 00]` on / `[00 00]` off. ear-web sends `0xF04F 00 00` before any normal preset
    write and `0xF04F 01 00` + `0xC04D FF` for Advanced [EW eq_two.js:30-50], [NC §7].
  * GET bands `0xC04D`, payload `[profile]` (`FF` = current) → `0x404D` [EW:bs:596-601].
  * SET bands `0xF050` [EW:bs:655-661]. Payload: `[profileIndex=0][bandCount][totalGain f32]` then per band
    13 bytes `[filterType(1)][gain f32][freq f32][Q f32]`; filterType 0 low-shelf, 1 peak, 2 high-shelf;
    ear-web always writes type 1 (peak) and totalGain = −max(0, gains) [EW:bs:579-661, 827-829].
    Default band centres (min–max): 55 (20–99), 110 (100–199), 220 (200–399), 440 (400–999), 1320 (1000–2999),
    3300 (3000–5999), 6600 (6000–11999), 13200 (12000–20000) Hz [EW:bs:583-592]. B171 config also carries
    `advancedEqTotalGain: -6` (unused by ear-web) [EW:cfg]. Q defaults: **[UNVERIFIED]** (read `0xC04D` first and edit).

### 7.2 CMF Buds Pro 2 B172/B187/B195 — "listening mode"

* GET `0xC050` → reply `0x4050`, payload[0] [EW:bs:446-450, 197-198], [BL:conf], [GB `equalizer_status 0xc050`].
* SET `0xF01D`, payload `[mode, 0x00]` [EW:bs:466-473], [BL:sock:613-617], [GB `equalizer 0xf01d`].
* Modes: `0` Dirac Opteo, `1` Rock, `2` Electronic, `3` Pop, `4` Enhance Vocals, `5` Classical, `6` Custom
  [EW MainControl_espeon.html:200-230, eq_espeon.js:34-70], [BL:B172,B187].
  [CONFLICT] Gadgetbridge: Rock 1 … Custom 6 identical, but **Dirac = 0x07** [GB NothingEqualizer]; something-x labels 0 "Balanced" [SX:120-128].
  → Use 0 for Dirac; accept 7 on read.
* ear-web blocks Dirac while LDAC is active on B172 (`diracOpteoSupport: 1`) — UI rule only [EW audio_codec.js:5-8]; B187/B195 have `diracOpteoSupport: 2` (no block).

### 7.3 Custom EQ (3-band parametric) — both models

* GET `0xC044` → `0x4044`; SET `0xF041` [EW:bs:823-845], [BL:conf], [SX:52-53]. Select it with preset 5 (B171) / mode 6 (B172) first.
* SET payload, 53 bytes [EW:bs:760-821], [NC §4]:

| Payload off | Size | Content |
|---|---|---|
| 0 | 1 | band count `03` |
| 1 | 4 | total gain f32 LE = −max(0, gains) (all ≤ 0 → −0.0 = `00 00 00 80`) |
| 5 | 13 | band A: type `01` peak, gain = **mid**, freq 980.0, Q 0.7 |
| 18 | 13 | band B: type `02` high-shelf, gain = **treble**, freq **3400.0 (B171) / 6900.0 (B172, B187, B195)**, Q 1.0 |
| 31 | 13 | band C: type `00` low-shelf, gain = **bass**, freq 140.0, Q 0.8 |
| 44 | 9 | zeros |

  Floats are IEEE-754 float32 little-endian. Gains are integers −6…+6 in the UI. Per-model freq/Q from
  `customEQ` in [EW:cfg] (B171 `freqHigh 3400`; B172/B187/B195 `freqHigh 6900`).
* Read: gains at payload offsets 6, 19, 32 (= frame 14, 27, 40) in order mid, treble, bass [EW:bs:905-923], [BL:sock:671-700], [SX:199].
* Decoding quirk (vendor app): if the 3 high bytes of the BE float are 0 and bit 7 of the low byte is set, clear it and negate [EW:bs:847-870], [BL:sock:628-652].
* [CONFLICT] BudsLink/something-x use a fixed template with peak Q 0.66 / high-shelf 3500 Hz and total = −max(|g|) [BL:sock:702-728], [SX:142-198]. Safest: **read `0xC044`, patch only the three gain floats + total gain, write back.**

### 7.4 Ultra Bass / bass enhance

* GET `0xC04E` → `0x404E` payload `[enabled, wireLevel]`; SET `0xF051` `[enabled, wireLevel]`; ACK/notify `0x7051`
  [EW:bs:528-563], [BL:sock:730-752], [GB ultra_bass].
* B171, B172, B187: 5 levels, wire = level × 2 (2,4,6,8,10); toggling keeps the level byte [EW:bs:538], [NC §7].
* B195 (and B186/B189/B197): 3 stages, wire 0, 5, 10 [EW:bs:528-532].
* GB read: `level = clamp(1..5, (wire+1)/2)` [GB handleUltraBassStatus].
* ear-web marks Ultra Bass mutually exclusive with spatial audio on models flagged `mutuallyExclusive` (UI rule) [EW:bs:475-493].

---

## 8. Other features

### 8.1 Low latency (game mode)
GET `0xC041` → `0x4041` payload[0]: `1` on, `2` off. SET `0xF040` `[01 00]` on / `[02 00]` off [EW:bs:1130-1169], [BL:sock:754-781], [BR].
Gadgetbridge sends a 1-byte payload `[01]`/`[02]` [GB encodeLowLatency] — the 2-byte form is the one captured from Nothing X [BR].
Notify/ACK `0x7040` (payload ≥ 2, byte0 = state) [BL:sock:239-241, 768-775]. Both models support it [EW], [BL].

### 8.2 In-ear detection
GET `0xC00E` → `0x400E`, enabled = **payload[2]** (frame byte 10) [EW:bs:1134-1142], [GB handleInEarStatus], [BL:sock:788-798].
SET `0xF004` `[01, 01, en]` (count=1, feature 1 = in-ear, value) [EW:bs:1151-1159], [ND:306-332], [BR]. ACK `0x7004`.
Both B171 and B172 [EW:bs:1124-1128], [BL].

### 8.3 Personalized ANC
GET `0xC020` → `0x4020` payload[0]; SET `0xF011 [en]` [EW:bs:1171-1193], [BL:conf].
ear-web enables it **only for B155**; BudsLink sets `personalizeAnc: true` for B171 [BL:B171]. B171 = **[UNVERIFIED]**; not on B172.
(B171 instead has Mimi "personal sound": GET `0xC022` → `0x4022` payload[0]; SET `0xF015 [en]` [EW:bs:1531-1572], cfg `mimi: 1`.)

### 8.4 Spatial audio
GET `0xC04F` → `0x404F` payload `[mode, head]`; SET `0xF052 [mode, head]` [EW:bs:1424-1506].
Modes by `spatialAudio` bitmask: 0x01 Head-tracked `01 01`, 0x02 Fixed `01 00`, 0x08 Concert `02 00`, 0x10 Theatre `03 00`, 0x20 Game `04 00`; off `00 00` [EW:bs:1424-1431], [NC §7].
* B172/B187/B195: `spatialAudio: 6` → Off/Fixed only (0x04 has no mode in ear-web) [EW:cfg], [NC §16].
* B171: no spatial-audio flag in config → not supported [EW:cfg].
* BudsLink and Gadgetbridge send a 1-byte `[01]`/`[00]` and read payload[0]==1 [BL:sock:903-921], [GB encodeSpatialAudio]. Two-byte form is ear-web's.

### 8.5 Ear-tip fit test
START `0xF014 [01]` (BudsLink sends `[00]` to stop) [EW:bs:1111-1115], [BL:sock:832-836], [BR].
Result push `0xE00D`, payload `[left, right]`: 0 good seal, 1 adjust, 2 not detected/error [EW:bs:1117-1122], [BL:sock:806-830], [BR].
Supported: B171, B172, B195 (also B155, B162, B173, B179, B184, B190, B197) [EW:bs:1112].

### 8.6 Find my buds (ring)
SET `0xF002`:
* Earbuds (B171, B172): `[side, on]`, side 0x02 left / 0x03 right, on 1/0 [EW:bs:925-952], [BL:sock:838-852].
* Headphones (B170, B164, B175, B186, B192): `[0x06, on]`; Ear (1) B181 legacy: `[on]` [EW:bs:927-939], [GB encodeFindDevice].
* Device replies/acks with `0x7002`; GB logs an incoming `0xF002`-normalised frame as "ack" [GB].

### 8.7 Gestures
* GET `0xC018` → `0x4018`. Payload `[N]` then N × `[device, button, gestureType, action]`
  [EW:bs:1210-1232], [GB handleTapConfigurationStatus], [BL:sock:859-884], [BR capture: 8 records for Ear (2)].
* SET `0xF003` payload `[0x01, device, button, gestureType, action]` (one record) [EW:bs:1234-1241], [GB encodeTouchOptions], [BL:sock:886-901], [BR].
* device: `0x02` left, `0x03` right, `0x04` case (CMF Buds Pro 2 smart knob), `0x06` headphone.
  button: `0x01` for buds; knob rows use 1 (nadamas also saw 0x09 rows on B187 [ND cpb2.json]).
* gestureType: `1` single, `2` double, `3` triple, `7` press-and-hold, `9` double-press-and-hold, `10` rotate [EW:cfg], [GB NothingTapType], [BL].
* Action codes [EW:cfg operations], [GB NothingTapAction], [BL], [BR]:

| Code | Action |
|---|---|
| 1 | no action |
| 2 | play/pause |
| 3 | answer call (fixed, not customisable) |
| 4 | hang up / decline (fixed) |
| 8 | skip back |
| 9 | skip forward |
| 10 (`0x0A`) | noise control: Transparency + NC + Off |
| 11 (`0x0B`) | voice assistant |
| 17 (`0x11`) | game mode (CMF case knob) |
| 18 (`0x12`) / 19 (`0x13`) | volume up / down |
| 20 (`0x14`) | noise control: NC + Off |
| 21 (`0x15`) | noise control: Transparency + Off |
| 22 (`0x16`) | noise control: Transparency + NC |
| 23 (`0x17`) | volume control (knob rotate) |
| 25 / 26 | case: answer+mute / hang-up (knob) |
| 29 | decline incoming call (B171 double-pinch, fixed) |
| 31 | "news" (Nothing news reporter, firmware-gated) |

  (Noise-control subset decoding [EW:gc decodeAncSelectorAction], [EW:dc:734-744].)
* Per-model allowed actions:
  * **B171** (pinch): double {8, 9, 11, 31*}; triple {8, 9, 11, 31*}; hold {10/20/21/22, 18, 19, 11, 31*}; double-hold {10/20/21/22, 18, 19, 11, 31*, 1}; factory default record set `08 0201020903010209 0201030803010308 0201071603010716 0201090103010901` (count 8) [EW:cfg B171], [EW twos.js], [BL:B171]. *31 needs fw ≥ 1.0.1.45.
  * **B172/B187** (tap): double {2, 8, 9, 11, 31, 1}; triple {8, 9, 11, 31, 1}; hold {10/20-22, 11, 31, 1}; double-hold {18, 19, 11, 31, 1}; case knob (device 4): single/double/triple {2, 9, 8, 11, 17, 31, 1}, hold {10, 11, 17, 31, 1}, rotate {23, 1} [EW:cfg B172/B187], [EW espeon.js], [BL:B172/B187]. ear-web never writes knob records; BudsLink does **[UNVERIFIED]**.
* The device accepts combos the official app hides (e.g. volume on double-pinch) [BR].

### 8.8 Hi-Res codec (LDAC/LHDC)
GET `0xC029` → `0x4029` payload[0]: 0 AAC/standard, 1 LHDC, 2 LDAC (out of range → 0). SET `0xF01C [idx]` [EW:bs:1385-1422].
Capability bitmask `highQualityAudio`: 0x02 LHDC, 0x04 LDAC [EW audio_codec.js:1-12]. B171 = 7 (AAC/LHDC/LDAC); B172/B187/B195 = 4 (AAC/LDAC) [EW:cfg].
The buds **reboot and drop RFCOMM**; ear-web waits 10 s then reconnects, retrying every 5 s ×6 [EW:bs:113-148, 300-306], [NC §1].
nadamas (Ear (3a)): reconnect A2DP after change; endpoint set is fixed at negotiation [ND ne3a.json notes].

### 8.9 Dual connection (multipoint)
* GET enabled `0xC027` → `0x4027` payload[0]; SET `0xF01A [en]` [EW:bs:1289-1306]. BudsLink notify `0xE006` [BL:conf] **[UNVERIFIED second source]**.
* B172/B187: `dualConnectionReboot: 1` (toggle reboots buds); B171 and B195: 0 [EW:cfg].
* List: GET `0xC028 [startIndex]` → `0x4028`: payload[2] = count; from payload[3] each entry `[flags][MAC×6][nameLen][name UTF-8]`; flags high nibble ≠0 = this host, low nibble ≠0 = connected; nameLen bit 7 = name clipped (BudsLink) [EW:bs:1319-1360], [BL:sock:949-997]. Page by re-requesting with the number of known devices (max 20 pages).
* Connect/disconnect a host: `0xF01B [1|0, MAC×6]` [EW:bs:1381-1383]. Event `0xE00E`: `[connected, MAC×6, needRefresh?]` [EW:bs:1362-1379], [BL:sock:999-1019].

---

## 9. Quirks & implementation notes

1. Always send CTRL `0x0160` + CRC; it is what every working client sends (draft's "drop all SETs" claim is [UNVERIFIED], from [SX] only).
2. Parse CTRL bit `0x20` on RX: replies have CRC, `0xE0xx` pushes with CTRL `0x0100` do not [BR], [GB], [BL].
3. Responses echo FSN; use one-in-flight with ~100 ms timeout (BudsLink) or fire-and-forget with 100 ms spacing (ear-web) [BL:sock:55-102], [EW:bs:76-102].
4. ear-web does not wait for/handle SET ACKs; read back state instead (e.g. `0xC01E` after `0xF00F`) — nadamas notes some firmware accepts writes for unsupported features and leaves state unchanged [ND:318-332].
5. Unsupported GETs on CMF Buds Pro 2 return an **empty payload** — treat LEN 0 as "unsupported" [ND cpb2.json].
6. Codec change and (on B172/B187) dual-connection toggle reboot the buds → expect socket drop and reconnect [EW:cfg], [EW:bs:300-306].
7. Pushes for the bud that didn't change may carry stale data (wear status) → re-query [SX:670-676, 990-997].
8. ear-web gates many reads by hard-coded model lists, not config (e.g. no `0xC00E` on B174/B175/B185/B186/B189) [NC §7].
9. RFCOMM channel varies (15 / 16) → SDP [BR], [EA].

---

## 10. Example frames (computed; FSN = `01`, CTRL `60 01`, CRC per §2.3)

Re-number byte 7 for other FSNs and recompute the CRC. "Models" = where sources say it applies.
Large payloads: custom EQ payloads are

* B171 bass+3/mid 0/treble−3 (53 B): `03 00 00 40 c0 01 00 00 00 00 00 00 75 44 33 33 33 3f 02 00 00 40 c0 00 80 54 45 00 00 80 3f 00 00 00 40 40 00 00 0c 43 cd cc 4c 3f 00 00 00 00 00 00 00 00 00`
* B172 same gains (high-shelf 6900 Hz): `03 00 00 40 c0 01 00 00 00 00 00 00 75 44 33 33 33 3f 02 00 00 40 c0 00 a0 d7 45 00 00 80 3f 00 00 00 40 40 00 00 0c 43 cd cc 4c 3f 00 00 00 00 00 00 00 00 00`
* B171 advanced EQ 8 flat bands at default centres, Q 1.0, total −0.0 (110 B) **[UNVERIFIED Q]**: `00 08 00 00 00 80` + 8 × `01 00000000 <freq f32> 0000803f`

Draft-FSN versions of the ANC sets (to replace the draft's broken goldens):

| Mode | Frame |
|---|---|
| Transparency (FSN cb) | `55 60 01 0f f0 03 00 cb 01 07 00 c5 af` (matches [BR]) |
| Off (FSN cd) | `55 60 01 0f f0 03 00 cd 01 05 00 c4 47` (matches [BR]) |
| High (FSN cf) | `55 60 01 0f f0 03 00 cf 01 01 00 c7 3f` |
| Mid (FSN d5) | `55 60 01 0f f0 03 00 d5 01 02 00 c0 d7` |
| Low (FSN d7) | `55 60 01 0f f0 03 00 d7 01 03 00 c0 ff` |
| Adaptive (FSN dd) | `55 60 01 0f f0 03 00 dd 01 04 00 c1 17` |

Full table (FSN = 01):

| Group | Operation | Cmd (wire) | Payload | Models | Complete frame, FSN=01 | Note |
|---|---|---|---|---|---|---|
| Session | GET protocol version | `0xC001` (`01 c0`) | (none) | B171, B172/B187 | `55 60 01 01 c0 00 00 01 24 df` | reply 0x4001 (ASCII); optional |
| Session | SET "activated" (after 0xC001 reply) | `0xF001` (`01 f0`) | (none) | B171, B172/B187 | `55 60 01 01 f0 00 00 01 2b df` | optional, BudsLink/something-x do it |
| Identity | GET model code | `0xC01C` (`1c c0`) | (none) | B171, B172/B187 | `55 60 01 1c c0 00 00 01 c8 dd` | reply 0x401C payload [lo,hi] e.g. 71 B1 |
| Identity | GET firmware | `0xC042` (`42 c0`) | (none) | B171, B172/B187 | `55 60 01 42 c0 00 00 01 61 10` | reply 0x4042 ASCII |
| Identity | GET serial / remote config | `0xC006` (`06 c0`) | (none) | B171, B172/B187 | `55 60 01 06 c0 00 00 01 91 1f` | reply 0x4006 CSV lines |
| Identity | SET UTC time (example epoch 0x6abc5100) | `0xF00A` (`0a f0`) | `6a bc 51 00` | B171, B172/B187 | `55 60 01 0a f0 04 00 01 6a bc 51 00 ef 42` | u32 BE unix seconds |
| Battery | GET battery | `0xC007` (`07 c0`) | (none) | B171, B172/B187 | `55 60 01 07 c0 00 00 01 ac df` |  |
| Wear | GET earphone/wear status | `0xC00A` (`0a c0`) | (none) | B171, B172/B187 | `55 60 01 0a c0 00 00 01 81 1e` | something-x/nadamas only |
| ANC | GET ANC | `0xC01E` (`1e c0`) | (none) | B171, B172/B187 | `55 60 01 1e c0 00 00 01 b1 1d` | ear-web form (no payload) |
| ANC | GET ANC (blog/something-x form) | `0xC01E` (`1e c0`) | `03` | B171, B172/B187 | `55 60 01 1e c0 01 00 01 03 9c 89` |  |
| ANC | SET ANC Off | `0xF00F` (`0f f0`) | `01 05 00` | B171, B172/B187 | `55 60 01 0f f0 03 00 01 01 05 00 fb 17` |  |
| ANC | SET ANC Transparency | `0xF00F` (`0f f0`) | `01 07 00` | B171, B172/B187 | `55 60 01 0f f0 03 00 01 01 07 00 fa 77` |  |
| ANC | SET ANC NC High | `0xF00F` (`0f f0`) | `01 01 00` | B171, B172/B187 | `55 60 01 0f f0 03 00 01 01 01 00 f9 d7` |  |
| ANC | SET ANC NC Mid | `0xF00F` (`0f f0`) | `01 02 00` | B171, B172/B187 | `55 60 01 0f f0 03 00 01 01 02 00 f9 27` |  |
| ANC | SET ANC NC Low | `0xF00F` (`0f f0`) | `01 03 00` | B171, B172/B187 | `55 60 01 0f f0 03 00 01 01 03 00 f8 b7` |  |
| ANC | SET ANC NC Adaptive | `0xF00F` (`0f f0`) | `01 04 00` | B171, B172/B187 | `55 60 01 0f f0 03 00 01 01 04 00 fa 87` | B172 Adaptive: see conflict |
| EQ (B171) | GET EQ preset | `0xC01F` (`1f c0`) | (none) | B171 | `55 60 01 1f c0 00 00 01 8c dd` |  |
| EQ (B171) | SET EQ preset Balanced | `0xF010` (`10 f0`) | `00 00` | B171 | `55 60 01 10 f0 02 00 01 00 00 27 a9` | send 0xF04F 00 00 first if Advanced was on |
| EQ (B171) | SET EQ preset Voice | `0xF010` (`10 f0`) | `01 00` | B171 | `55 60 01 10 f0 02 00 01 01 00 26 39` | send 0xF04F 00 00 first if Advanced was on |
| EQ (B171) | SET EQ preset More Treble | `0xF010` (`10 f0`) | `02 00` | B171 | `55 60 01 10 f0 02 00 01 02 00 26 c9` | send 0xF04F 00 00 first if Advanced was on |
| EQ (B171) | SET EQ preset More Bass | `0xF010` (`10 f0`) | `03 00` | B171 | `55 60 01 10 f0 02 00 01 03 00 27 59` | send 0xF04F 00 00 first if Advanced was on |
| EQ (B171) | SET EQ preset Custom | `0xF010` (`10 f0`) | `05 00` | B171 | `55 60 01 10 f0 02 00 01 05 00 24 f9` | send 0xF04F 00 00 first if Advanced was on |
| EQ (B171) | GET advanced-EQ enabled | `0xC04C` (`4c c0`) | (none) | B171 | `55 60 01 4c c0 00 00 01 08 d1` |  |
| EQ (B171) | SET advanced EQ ON | `0xF04F` (`4f f0`) | `01 00` | B171 | `55 60 01 4f f0 02 00 01 01 00 89 3c` |  |
| EQ (B171) | SET advanced EQ OFF | `0xF04F` (`4f f0`) | `00 00` | B171 | `55 60 01 4f f0 02 00 01 00 00 88 ac` |  |
| EQ (B171) | GET advanced EQ bands (current profile) | `0xC04D` (`4d c0`) | `ff` | B171 | `55 60 01 4d c0 01 00 01 ff 90 ab` |  |
| EQ (B172) | GET listening mode | `0xC050` (`50 c0`) | (none) | B172/B187 | `55 60 01 50 c0 00 00 01 d9 13` |  |
| EQ (B172) | SET listening mode Dirac Opteo | `0xF01D` (`1d f0`) | `00 00` | B172/B187 | `55 60 01 1d f0 02 00 01 00 00 fb 69` | Gadgetbridge uses 0x07 for Dirac (conflict) |
| EQ (B172) | SET listening mode Rock | `0xF01D` (`1d f0`) | `01 00` | B172/B187 | `55 60 01 1d f0 02 00 01 01 00 fa f9` |  |
| EQ (B172) | SET listening mode Electronic | `0xF01D` (`1d f0`) | `02 00` | B172/B187 | `55 60 01 1d f0 02 00 01 02 00 fa 09` |  |
| EQ (B172) | SET listening mode Pop | `0xF01D` (`1d f0`) | `03 00` | B172/B187 | `55 60 01 1d f0 02 00 01 03 00 fb 99` |  |
| EQ (B172) | SET listening mode Enhance Vocals | `0xF01D` (`1d f0`) | `04 00` | B172/B187 | `55 60 01 1d f0 02 00 01 04 00 f9 a9` |  |
| EQ (B172) | SET listening mode Classical | `0xF01D` (`1d f0`) | `05 00` | B172/B187 | `55 60 01 1d f0 02 00 01 05 00 f8 39` |  |
| EQ (B172) | SET listening mode Custom | `0xF01D` (`1d f0`) | `06 00` | B172/B187 | `55 60 01 1d f0 02 00 01 06 00 f8 c9` |  |
| Custom EQ | GET custom EQ | `0xC044` (`44 c0`) | (none) | B171, B172/B187 | `55 60 01 44 c0 00 00 01 e9 10` |  |
| Custom EQ | SET custom EQ bass+3 mid0 treble-3 (B171, HS 3400 Hz) | `0xF041` (`41 f0`) | `53 bytes (see §6)` | B171 | `55 60 01 41 f0 35 00 01 03 00 00 40 c0 01 00 00 00 00 00 00 75 44 33 33 33 3f 02 00 00 40 c0 00 80 54 45 00 00 80 3f 00 00 00 40 40 00 00 0c 43 cd cc 4c 3f 00 00 00 00 00 00 00 00 00 2a a2` |  |
| Custom EQ | SET custom EQ flat (B171) | `0xF041` (`41 f0`) | `53 bytes (see §6)` | B171 | `55 60 01 41 f0 35 00 01 03 00 00 00 80 01 00 00 00 00 00 00 75 44 33 33 33 3f 02 00 00 00 00 00 80 54 45 00 00 80 3f 00 00 00 00 00 00 00 0c 43 cd cc 4c 3f 00 00 00 00 00 00 00 00 00 1e e5` |  |
| Custom EQ | SET custom EQ bass+3 mid0 treble-3 (B172, HS 6900 Hz) | `0xF041` (`41 f0`) | `53 bytes (see §6)` | B172/B187 | `55 60 01 41 f0 35 00 01 03 00 00 40 c0 01 00 00 00 00 00 00 75 44 33 33 33 3f 02 00 00 40 c0 00 a0 d7 45 00 00 80 3f 00 00 00 40 40 00 00 0c 43 cd cc 4c 3f 00 00 00 00 00 00 00 00 00 53 1b` |  |
| Custom EQ | SET custom EQ flat (B172) | `0xF041` (`41 f0`) | `53 bytes (see §6)` | B172/B187 | `55 60 01 41 f0 35 00 01 03 00 00 00 80 01 00 00 00 00 00 00 75 44 33 33 33 3f 02 00 00 00 00 00 a0 d7 45 00 00 80 3f 00 00 00 00 00 00 00 0c 43 cd cc 4c 3f 00 00 00 00 00 00 00 00 00 67 5c` |  |
| EQ (B171) | SET advanced EQ 8 bands flat, Q=1.0 (example) | `0xF050` (`50 f0`) | `110 bytes (see §6)` | B171 | `55 60 01 50 f0 6e 00 01 00 08 00 00 00 80 01 00 00 00 00 00 00 5c 42 00 00 80 3f 01 00 00 00 00 00 00 dc 42 00 00 80 3f 01 00 00 00 00 00 00 5c 43 00 00 80 3f 01 00 00 00 00 00 00 dc 43 00 00 80 3f 01 00 00 00 00 00 00 a5 44 00 00 80 3f 01 00 00 00 00 00 40 4e 45 00 00 80 3f 01 00 00 00 00 00 40 ce 45 00 00 80 3f 01 00 00 00 00 00 40 4e 46 00 00 80 3f 32 16` | [UNVERIFIED Q/default values] |
| Bass | GET ultra bass | `0xC04E` (`4e c0`) | (none) | B171, B172/B187 | `55 60 01 4e c0 00 00 01 71 11` |  |
| Bass | SET ultra bass ON level 1 | `0xF051` (`51 f0`) | `01 02` | B171, B172/B187 | `55 60 01 51 f0 02 00 01 01 02 f6 fc` | B195 uses 3 stages 0/5/10 |
| Bass | SET ultra bass ON level 2 | `0xF051` (`51 f0`) | `01 04` | B171, B172/B187 | `55 60 01 51 f0 02 00 01 01 04 76 fe` | B195 uses 3 stages 0/5/10 |
| Bass | SET ultra bass ON level 3 | `0xF051` (`51 f0`) | `01 06` | B171, B172/B187 | `55 60 01 51 f0 02 00 01 01 06 f7 3f` | B195 uses 3 stages 0/5/10 |
| Bass | SET ultra bass ON level 4 | `0xF051` (`51 f0`) | `01 08` | B171, B172/B187 | `55 60 01 51 f0 02 00 01 01 08 76 fb` | B195 uses 3 stages 0/5/10 |
| Bass | SET ultra bass ON level 5 | `0xF051` (`51 f0`) | `01 0a` | B171, B172/B187 | `55 60 01 51 f0 02 00 01 01 0a f7 3a` | B195 uses 3 stages 0/5/10 |
| Bass | SET ultra bass OFF (keep level 3) | `0xF051` (`51 f0`) | `00 06` | B171, B172/B187 | `55 60 01 51 f0 02 00 01 00 06 f6 af` |  |
| Latency | GET low latency | `0xC041` (`41 c0`) | (none) | B171, B172/B187 | `55 60 01 41 c0 00 00 01 25 10` |  |
| Latency | SET low latency ON | `0xF040` (`40 f0`) | `01 00` | B171, B172/B187 | `55 60 01 40 f0 02 00 01 01 00 76 3c` |  |
| Latency | SET low latency OFF | `0xF040` (`40 f0`) | `02 00` | B171, B172/B187 | `55 60 01 40 f0 02 00 01 02 00 76 cc` |  |
| In-ear | GET in-ear detection | `0xC00E` (`0e c0`) | (none) | B171, B172/B187 | `55 60 01 0e c0 00 00 01 70 de` |  |
| In-ear | SET in-ear detection ON | `0xF004` (`04 f0`) | `01 01 01` | B171, B172/B187 | `55 60 01 04 f0 03 00 01 01 01 01 79 a4` |  |
| In-ear | SET in-ear detection OFF | `0xF004` (`04 f0`) | `01 01 00` | B171, B172/B187 | `55 60 01 04 f0 03 00 01 01 01 00 b8 64` |  |
| Fit test | START ear-tip fit test | `0xF014` (`14 f0`) | `01` | B171, B172/B187 | `55 60 01 14 f0 01 00 01 01 5d e6` |  |
| Ring | RING left ON | `0xF002` (`02 f0`) | `02 01` | B171, B172/B187 | `55 60 01 02 f0 02 00 01 02 01 d5 08` |  |
| Ring | RING left OFF | `0xF002` (`02 f0`) | `02 00` | B171, B172/B187 | `55 60 01 02 f0 02 00 01 02 00 14 c8` |  |
| Ring | RING right ON | `0xF002` (`02 f0`) | `03 01` | B171, B172/B187 | `55 60 01 02 f0 02 00 01 03 01 d4 98` |  |
| Ring | RING right OFF | `0xF002` (`02 f0`) | `03 00` | B171, B172/B187 | `55 60 01 02 f0 02 00 01 03 00 15 58` |  |
| Gestures | GET gestures | `0xC018` (`18 c0`) | (none) | B171, B172/B187 | `55 60 01 18 c0 00 00 01 39 1d` |  |
| Gestures | SET left double-pinch = skip forward | `0xF003` (`03 f0`) | `01 02 01 02 09` | B171 | `55 60 01 03 f0 05 00 01 01 02 01 02 09 d2 f9` |  |
| Gestures | SET right pinch-hold = noise control (T+NC) | `0xF003` (`03 f0`) | `01 03 01 07 16` | B171, B172/B187 | `55 60 01 03 f0 05 00 01 01 03 01 07 16 91 9d` |  |
| Gestures | SET left double-pinch-hold = volume up | `0xF003` (`03 f0`) | `01 02 01 09 12` | B171, B172/B187 | `55 60 01 03 f0 05 00 01 01 02 01 09 12 95 c2` |  |
| Gestures | SET left double-tap = play/pause | `0xF003` (`03 f0`) | `01 02 01 02 02` | B172/B187 | `55 60 01 03 f0 05 00 01 01 02 01 02 02 93 3e` |  |
| Gestures | SET case knob single press = game mode | `0xF003` (`03 f0`) | `01 04 01 01 11` | B172/B187 | `55 60 01 03 f0 05 00 01 01 04 01 01 11 d2 8b` | [UNVERIFIED; never sent by ear-web] |
| Codec | GET audio codec | `0xC029` (`29 c0`) | (none) | B171, B172/B187 | `55 60 01 29 c0 00 00 01 44 d9` |  |
| Codec | SET codec AAC (reboots) | `0xF01C` (`1c f0`) | `00` | B171, B172/B187 | `55 60 01 1c f0 01 00 01 00 9d 6e` | risky |
| Codec | SET codec LHDC (reboots) | `0xF01C` (`1c f0`) | `01` | B171 | `55 60 01 1c f0 01 00 01 01 5c ae` | risky |
| Codec | SET codec LDAC (reboots) | `0xF01C` (`1c f0`) | `02` | B171, B172/B187 | `55 60 01 1c f0 01 00 01 02 1c af` | risky |
| Spatial | GET spatial audio | `0xC04F` (`4f c0`) | (none) | B172/B187 | `55 60 01 4f c0 00 00 01 4c d1` |  |
| Spatial | SET spatial OFF | `0xF052` (`52 f0`) | `00 00` | B172/B187 | `55 60 01 52 f0 02 00 01 00 00 45 ad` |  |
| Spatial | SET spatial Fixed | `0xF052` (`52 f0`) | `01 00` | B172/B187 | `55 60 01 52 f0 02 00 01 01 00 44 3d` |  |
| Dual | GET dual connection enabled | `0xC027` (`27 c0`) | (none) | B171, B172/B187 | `55 60 01 27 c0 00 00 01 2d 18` |  |
| Dual | SET dual connection ON | `0xF01A` (`1a f0`) | `01` | B171, B172/B187 | `55 60 01 1a f0 01 00 01 01 5c c8` | B172 reboots |
| Dual | SET dual connection OFF | `0xF01A` (`1a f0`) | `00` | B171, B172/B187 | `55 60 01 1a f0 01 00 01 00 9d 08` | B172 reboots |
| Dual | GET paired-host list from index 0 | `0xC028` (`28 c0`) | `00` | B171, B172/B187 | `55 60 01 28 c0 01 00 01 00 d9 1e` |  |
| Dual | SET connect host AA-BB-CC-DD-EE-FF | `0xF01B` (`1b f0`) | `01 aa bb cc dd ee ff` | B171, B172/B187 | `55 60 01 1b f0 07 00 01 01 aa bb cc dd ee ff ae 0b` | risky |
| Dual | SET disconnect host AA-BB-CC-DD-EE-FF | `0xF01B` (`1b f0`) | `00 aa bb cc dd ee ff` | B171, B172/B187 | `55 60 01 1b f0 07 00 01 00 aa bb cc dd ee ff be cb` | risky |
| Personal sound | GET Mimi personal sound | `0xC022` (`22 c0`) | (none) | B171 | `55 60 01 22 c0 00 00 01 e1 18` |  |
| Personal sound | SET Mimi ON | `0xF015` (`15 f0`) | `01` | B171 | `55 60 01 15 f0 01 00 01 01 5c 37` |  |
| Personal sound | SET Mimi OFF | `0xF015` (`15 f0`) | `00` | B171 | `55 60 01 15 f0 01 00 01 00 9d f7` |  |
| Personalized ANC | GET personalized ANC | `0xC020` (`20 c0`) | (none) | B155 (B171 per BudsLink only) | `55 60 01 20 c0 00 00 01 98 d8` | [B171 UNVERIFIED] |
| Personalized ANC | SET personalized ANC ON | `0xF011` (`11 f0`) | `01` | B155 (B171 per BudsLink only) | `55 60 01 11 f0 01 00 01 01 5d b3` | [B171 UNVERIFIED] |
| Personalized ANC | SET personalized ANC OFF | `0xF011` (`11 f0`) | `00` | B155 (B171 per BudsLink only) | `55 60 01 11 f0 01 00 01 00 9c 73` | [B171 UNVERIFIED] |

## 11. Open items / unverified

* B172 Adaptive ANC (0x04): ear-web/BudsLink yes; Gadgetbridge and nadamas (real B187) omit it.
* CMF Dirac Opteo value: 0x00 (ear-web, BudsLink) vs 0x07 (Gadgetbridge).
* Personalized ANC (0xC020/0xF011) on B171: BudsLink only.
* Case-knob gesture writes (device 0x04) on B172: BudsLink only, never sent by ear-web.
* Advanced EQ (0xF050) default Q values and whether advancedEqTotalGain -6 must be used on B171.
* Whether a second RFCOMM client (phone + PC) can share the control channel.
* 0xC00A wear-status bit layout: something-x only.
* SKU digit 71: B162 (something-x) vs B171 (EarA).
* Fast Pair message header bytes 0..3 meaning (from the Fast Pair spec, not checked by ear-web).
