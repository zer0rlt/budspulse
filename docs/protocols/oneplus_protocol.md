# OnePlus / OPPO / realme "HeyMelody" earbud control protocol (OPOv1, `0xAA` framing): byte-level spec

Research date: 2026-11-15. Written to replace section 2.2 of `claude_code_earbuds_widget_prompt.md`. Every fact below cites a file in real source code or a capture log. If a fact has no source, it is marked **GUESS**.

## 0. Sources and evidence levels

### Sources (clones pinned at the commit read)

| Tag | Source | Commit / date | Notes |
|---|---|---|---|
| **QB** | [spizganed/QuickBuds](https://github.com/spizganed/QuickBuds) `docs/PROTOCOL.md` (§n = section), `app/src/main/kotlin/.../protocol/*.kt`, `core/src/*.rs`, `desktop-tauri/src/bt.rs`, `app/src/main/assets/models.json` | `8f23064` 2026-11-14 | GPL-3.0. This is the best source. It is a working **Tauri v2 + Rust** desktop app for Windows and Linux, plus Android. It covers 143 models and is fully hardware-tested on OnePlus Buds 4. Owners confirmed Buds Pro 3 and Nord Buds 3 Pro. Its own tags: `[CAPTURE]`/`[TESTED]` (hardware), `[FIRMWARE]` (read from a firmware image), `[VENDOR]` (HeyMelody behaviour/data), `[GUESS]`. |
| **MC** | [chiragverma11/melodyctl](https://github.com/chiragverma11/melodyctl) `docs/protocol/*.md`, `whitelist.json` (decrypted HeyMelody whitelist, 85 models), `melodyctl/data/profiles.json` | `9d15fc5` 2026-10-20 | Hardware-verified on OPPO Enco Buds (ETI81, `062410`) only. Its APK notes are cited as `[apk:...]`. |
| **NP / ND** | [nic0manz/oneplus_buds3_pro_python](https://github.com/nic0manz/oneplus_buds3_pro_python) `op_buds3_pro.py`; [nic0manz/oneplus_buds3_pro_dotnet](https://github.com/nic0manz/oneplus_buds3_pro_dotnet) `OnePlusBudsPro3/BudsConnection.cs` | `faec7e5` / `ecb5ab8` 2026-06-23 | Windows tools used by the author on Buds Pro 3: AF_BTH RFCOMM, channel 15, hardcoded frames. This is where the draft's HELLO/REGISTER came from. |
| **OB** | [himanshudev28/OneBuds](https://github.com/himanshudev28/OneBuds) `Documentation/ProtocolSources.md`, `Documentation/PhaseB-Report.md` | `d3acc05` 2026-11-03 | macOS app. Has SDP/GATT dumps and query traffic observed on a real **OnePlus Buds 3**. |
| **GB** | Gadgetbridge (codeberg `Freeyourgadget/Gadgetbridge`) `app/src/main/java/nodomain/freeyourgadget/gadgetbridge/service/devices/oppo/**` | `3c30da0` 2026-11-14 | Supports OPPO Enco Air/Air2/Buds2 and realme Buds; no OnePlus model. Its ANC enum is the realme table. |
| **BL** | [maniacx/BudsLink](https://github.com/maniacx/BudsLink) `src/lib/devices/opoBuds/*.js` | `f4f98c0` 2026-11-13 | GNOME. RFCOMM via a BlueZ profile on UUID `079A`; includes a OnePlus Buds 4 (`065414`) config. |
| **OOB** | [vedanth-jadhav/open-oneplus-buds](https://github.com/vedanth-jadhav/open-oneplus-buds) `Sources/BudsCore/*.swift` | `0d66959` 2026-03-13 | macOS BLE GATT client used on Nord Buds 3 Pro. |
| **AB** | Blog <https://aasheesh.vercel.app/blog/oneplus-buds> + [AasheeshLikePanner/cracked-oneplus-buds](https://github.com/AasheeshLikePanner/cracked-oneplus-buds) `nordbuds.swift` | `6320765` 2026-02-27 | The original BLE write-up for Nord Buds 3 Pro. Its field interpretations are **largely wrong** (see §9). |
| **GZ** | GazzasaurusRex/oneplus-buds-omarchy, as summarised in MC `docs/research/prior-art/gazzasaurusrex.md` | `abe70f4` | Linux, hardware-verified on Buds Pro `060C14` and Buds Pro 2 `062014`. |

### Evidence levels used below

- **HW**: seen on real hardware in a capture or test log by the cited project (model named).
- **CODE**: implemented in open-source code people use, but no published capture for that model.
- **VENDOR**: taken from HeyMelody's model whitelist or decompiled app behaviour, as quoted by QB or MC. Not tested on hardware.
- **GUESS**: unverified, or my own inference.

---

## 1. Frame format

### 1.1 Layout (both directions; identical on RFCOMM and BLE)

```
off  size  field
0    1     0xAA                         start of frame
1    1-3   TotalLen  (LEB128 varint)    = number of bytes AFTER this field
+0   1     Slice     00 whole | 01 first | 02 middle | 03 last slice
+1   1     0x00      reserved
+2   2     Cmd       u16 little-endian
+4   1     Seq
+5   2     PayLen    u16 little-endian
+7   n     Payload
```

With a one-byte length (every frame below 128 bytes), the header is a fixed 9 bytes:
`[0]=AA [1]=TotalLen [2]=00 [3]=00 [4]=cmd_lo [5]=cmd_hi [6]=seq [7]=plen_lo [8]=plen_hi [9..]=payload`.

- **TotalLen = 7 + PayLen**. It counts every byte after itself, so a whole frame is `TotalLen + 2` bytes (one-byte length). Sources: **HW** QB §2; GB `OppoHeadphonesSupport.java` `encodeCommand()` (`totalLength = 7 + payload.length`, LEB128); GZ ("frame length is total_length + 2"); MC `framing.md`.
  - Check: `AA 07 00 00 00 01 01 00 00` (handshake, PayLen 0) is 9 bytes, and TotalLen is 7.
- **TotalLen is LEB128** (7 bits per byte, low bits first, bit 7 = more bytes follow). A 124-byte payload is sent as `AA 83 01 ...` (131). A 250-byte reply arrives as `AA 81 02` (257). **HW** QB §2. GB also encodes with `LEB128Utils.encodeUnsigned`. Only big transfers (Golden Sound, ear scan) exceed 127 bytes.
- **No checksum or CRC.** **HW** QB §2; GZ; AB "Mistake #3".
  - One exception: if bit 3 (`0x08`) of the Slice byte is set, 2 extra bytes follow the payload, probably a check value. This was read from the Buds 4 firmware image (**VENDOR/FIRMWARE**, QB §2). A host never sets it.
- **Cmd is u16 LE.** `0x0106` goes on the wire as `06 01`. The high byte is the command class (QB §4):

| Cmd range | Class | Reply |
|---|---|---|
| `0x01xx` | query | `0x81xx` |
| `0x02xx` | event list / subscribe / push (`0x0204`) | `0x82xx` |
| `0x03xx` | firmware upgrade (do not use) | |
| `0x04xx` | write / set | `0x84xx` |
| `0x05xx` | **requests from the buds** to the host; some must be answered (§3.3) | host sends `0x85xx` |
| `0x0Fxx`, `0xEFxx` | private / capability-only | |

  - Reply = request cmd **| 0x8000**. Example: `0x0106` → `0x8106`. **HW** QB §3; GB `CMD_MASK_RESPONSE = 0x8000`; MC `framing.md`.
- **Seq**: the sender picks it, and the reply echoes it. Match replies on `(seq, cmd & 0x7FFF)`. **HW** QB §3; GZ ("0x8404 and 0x810c responses echo the request sequence").
  - Unsolicited pushes from the buds use **Seq = `0xFF`** (**HW** QB §2).
  - The increment does not matter. GB uses 0,1,2…; BL uses 1..250; QB uses 01..BF plus fixed seqs (`F0` for battery, `F2` for wear, `00` for the status read).
  - Do not use `FF` for requests. **GUESS**, but every client avoids it.
  - Exception: realme Buds T100/T110 answer with their own seq ("seqless", QB "How the buds behave").
- **Payload**: replies to queries and writes start with a **status byte**; `00` = OK (§4.0).
- **Byte 2** is not always `00` on receive:
  - GB `handleSingleResponse` accepts `00 00` or `04 00` ("0 on oppo, 4 on realme?").
  - QB reports realme T200 Lite/T200x send `04` in byte 2.
  - QB treats only values 1-3 as slice markers.
  - Rule: send `00 00`. On receive, accept any value that is not 1-3 and parse as a whole frame.

### 1.2 Splitting a byte stream into frames (receiver algorithm)

From QB `Wire.kt` `OppoPacketFramer` and `core/src/wire.rs`, GZ, and MC `framing.md`:

1. Append incoming bytes to a buffer. **An RFCOMM read or a BLE notification can hold several frames or part of one** (HW: QB; OB §4 "one BLE notification can carry several concatenated AA frames").
2. Throw away bytes before the first `AA`.
3. Read the LEB128 TotalLen (at most 3 bytes; if there is no terminator after 3 bytes, drop the `AA` and rescan). Wait until `1 + lenBytes + TotalLen` bytes are present.
4. Sanity check: TotalLen must be ≥ 7 for a whole frame (≥ 4 for a slice), and the frame must be ≤ 2048 bytes. Otherwise drop one byte and rescan.
5. **The outer TotalLen is authoritative.** GZ saw the inner PayLen disagree:
   - a `0x8100` reply was 1 byte short;
   - a `0x0204` push was 3 bytes longer.

   Concatenated frames still split exactly on TotalLen. Clamp the payload to `min(PayLen, bytes present)` (QB `Wire.kt` `message()` does the same).
6. Slices (Slice byte 1/2/3): the data after `Slice 00 Fsn` is joined until slice `03`, then parsed as one message. Hosts never need to send slices below 660 bytes. **VENDOR** QB §2 "Slicing".

### 1.3 RFCOMM vs BLE differences

- The framing is identical. A BLE frame captured on Nord Buds 3 Pro (OOB `OPOProtocol.swift` comment) parses with exactly the layout above:
  `AA 0F 00 00 06 81 25 08 00 00 03 01 64 02 64 03 00` = `0x8106` battery reply, seq `25`, PayLen 8, `00 03 01=100 02=100 03=0`.
- BLE only: one notification is at most ATT_MTU−3 bytes. Split writes over that size, and reassemble across notifications (OB §8 "chunk to MTU"). **CODE**.
- Seq increments of 2 (`40,42,44`) are **not** a BLE rule. They are what one HeyMelody session happened to use (AB).

---

## 2. Transports

### 2.1 Classic Bluetooth RFCOMM/SPP (use this; it is the HeyMelody primary)

| Item | Value | Evidence |
|---|---|---|
| Vendor SDP service UUID | **`0000079A-D102-11E1-9B23-00025B00A5A5`**. Try it first. | **HW**: Buds 4 (QB §1), Buds 3 (OB PhaseB §4.2, SDP name `"enco link "`), Buds Pro 3 (QB §1 firmware table), OPPO ETI81 (MC `transport.md`). GB `OppoUuid.VENDOR_RFCOMM`; BL `OpoBudsUUID`. |
| Legacy UUID | `00001107-D102-11E1-9B23-00025B00A5A5`. Try it second. | **HW** on the original OnePlus Buds Pro `060C14` (GZ). It is absent on Buds 3 (OB) and never connects on Buds 4 (QB §1). |
| Plain SPP `00001101-0000-1000-8000-00805F9B34FB` | **Not this protocol** on OnePlus: it is the chip vendor's tool port (ch 12) and gets no reply (QB §1, Nord Buds 4 Pro). Exception: GB uses it for realme Buds Air6 Pro and T200 (`RealmeBudsAir6ProCoordinator.ctrlUuid()`). | HW (QB), CODE (GB) |
| RFCOMM channel | **Take it from SDP; do not hardcode it.** | see below |

Observed channels:

- `079A` = **channel 15** on Buds 4, Buds Pro 3 and Buds 3. Sources: QB §1 `[FIRMWARE]`, OB PhaseB `[observed]`, NP/ND hardcode 15 for Pro 3.
- `079A` = **channel 12** on OPPO Enco Buds ETI81 (MC).
- `1107` = channel 5 on one model (OB §3).
- Fallback if SDP fails: try 15, then 12, then 1..30 (MC transport.md probe order). **CODE**.

- **Windows (Rust, `windows-sys`)**, proven in QB `desktop-tauri/src/bt.rs::connect_uuid`:
  - Call `socket(AF_BTH, SOCK_STREAM, BTHPROTO_RFCOMM)`.
  - Fill `SOCKADDR_BTH { addressFamily: AF_BTH, btAddr, serviceClassId: GUID(079A…), port: 0 }`. Port 0 makes **Windows run the SDP lookup itself**. Then `connect()`. If that fails, retry with `1107`.
  - Set `SO_RCVTIMEO` to 50 ms for a polling read loop.
  - List paired devices with `BluetoothFindFirstDevice`/`BluetoothEnumerateInstalledServices`, matching `079A`/`1107`.
  - Optional: `BluetoothSetServiceState(A2DP 110B / HFP 111E, ENABLE)` asks Windows to reconnect audio (`bring_up()`).
  - ND does the same with a raw SOCKADDR_BTH, but passes port = 15 directly.
- **Linux**: two options.
  - Raw `AF_BLUETOOTH/BTPROTO_RFCOMM` socket. QB `bt.rs` (linux mod) does its own SDP `ServiceSearchAttributeRequest` for the channel and gets the paired list from BlueZ D-Bus.
  - Or register a BlueZ `Profile1` for UUID `079A` (BL `opoBudsDevice.js`).
- **One RFCOMM link per host.** A second open on the same host fails with EBUSY (errno 16) (MC transport.md; GZ). The phone's HeyMelody link is separate: in **dual connection the PC and the phone each hold their own `079A` link at the same time**. Sources:
  - QB §18: "with dual connection each host reads its own value. Buds 4: phone `18=1`… PC `18=0`… at the same time" `[CAPTURE]`.
  - QB §1: Nord Buds 4 Pro's `079A` server "takes 2 links".
  - MageshwarSV/QuickBuds README: "With Dual connection on, the phone app and the desktop app work at the same time."
- **The link dies with the lid.** Close the case with both buds inside and the socket drops 1-3 s later. The buds are unreachable until the lid opens; reconnect and re-subscribe after that. **HW** QB §13, MC `claims-matrix.md` #45.
- Idle or asleep buds give `Host is down` (errno 112/113); retrying wakes them (MC). QB uses a 12 s connect watchdog, 5 socket attempts, and 8 reconnect attempts (QB §1).
- The Google Fast Pair Message Stream (`df21fe2c-2515-4fdb-8886-f12c4d67927c`, RFCOMM) is a separate service. It can give battery even while another app holds `079A`, but it is usually held by Play Services on Android. On PC it is a fallback only. **HW** QB §1.

### 2.2 BLE GATT (fallback only; do not make it primary on Windows)

| Item | Value | Evidence |
|---|---|---|
| Service | `0000079A-D102-11E1-9B23-00025B00A5A5` | **HW** observed on Buds 3 (OB PhaseB §4.3); CODE OOB/AB (Nord Buds 3 Pro) |
| Write char | `0100079A-D102-11E1-9B23-00025B00A5A5`. Properties: writeWithoutResponse + write. **Use Write Without Response.** | Props **HW** (OB). The claim that "with-response writes are silently ignored" comes from AB/OOB only (`BudsClient.swift` L476 comment). Not independently verified. |
| Notify char | `0200079A-D102-11E1-9B23-00025B00A5A5`. Properties: read + notify, CCCD present. Enable notifications **before** sending anything. | HW props (OB); CODE (OOB) |
| `FE2C` service, chars `FE2C1233…FE2C1238-8366-4814-8EB0-01DE32100BEA` | **This is Google Fast Pair, not OPO.** Do not write to it. | HW observed char set (OB §9 C7) |
| `0000079C-…` service | Skip it: it can require encryption and force a disconnect (OOB `BudsClient.swift` L754-770). | CODE |

- The draft's "**MUST subscribe to `0200079A` and `FE2C123A` or the round-trip fails**" comes from AB's anecdote only. On the same night AB also changed timing and handshake. OB's GATT dump of Buds 3 has **no `FE2C123A`** (only `1233`–`1238`).
  - Subscribing to every notify characteristic is harmless, so a BLE client may do it. Treat it as **UNVERIFIED** folklore.
  - The only proven need is the CCCD on `0200079A` (QB/OB/OOB all wait for it).
- Which bearer answers when both exist: on Buds 3 RFCOMM answered first and BLE was not exercised (OB PhaseB §6).
  - QB (Buds 4, Pro 3, Nord Buds 3 Pro, Windows/Linux/Android) uses **only RFCOMM**: "Classic Bluetooth RFCOMM / SPP, not BLE" (QB §1).
  - melodyctl's 85-model whitelist: every entry has `supportSpp: true`; there are no BLE-only devices (MC open-questions OQ-10).
- **Recommendation for this app: RFCOMM on both Windows and Linux; BLE only as an experimental fallback.** WinRT GATT to a dual-mode audio headset is also unreliable in practice (**GUESS**).
- BLE advertisement (no connection needed), useful for detecting the model and case battery:
  - Manufacturer data has company id `0x079A` (OPPO) or `0x072F` (OnePlus); realme uses `0xA408`.
  - Bytes 0-2 are the product id, LE. Buds 3 example: `9A 07 14 3C 06 …` → `063C14` (**HW** OB PhaseB §4.3).
  - The full field table (flags, battery `(b>>1)&0x7F` when bit0 = valid) is in QB §13 "Other battery sources" (**VENDOR**).

---

## 3. Session start

### 3.1 There is no authentication. HELLO/REGISTER were misread captures.

The draft's two "auth" frames decode cleanly with §1.1. I rebuilt both byte-for-byte with a frame encoder (`0x0100` handshake, and `0x8500` = time-request answer):

| Draft "name" | Bytes | Real meaning |
|---|---|---|
| HELLO | `AA 07 00 00 00 01 23 00 00` **`12`** | `0x0100` **handshake / capability query**, seq `23`, PayLen 0. The trailing `12` is **outside the frame** (TotalLen 7 → frame ends after 9 bytes). It is a stray byte that the buds' framer skips while searching for `AA`. |
| REGISTER + "token" | `AA 0C 00 00 00 85 41 05 00 00 B5 50 A0 69` | Cmd **`0x8500`** = the host's **answer to the buds' time request `0x0500`**, seq `41`, PayLen 5. Payload `00` (status) + u32 LE `0x69A050B5` = **1772114101 = 2026-02-26 13:55:01 UTC**, the day before AB's repo was created. **It is a replayed timestamp, not a token.** (OB ProtocolSources §5 reached the same conclusion. The arithmetic was re-checked here.) |
| BLE "ANC query" | `AA 09 00 00 04 82 44 02 00 00 F2` | Cmd **`0x8204`**, payload `00 F2` = the host's **acknowledgement of a `0x0204` push with event `F2`** (connected-devices list). It is **not** a query. The real ANC query is `0x010C 01 01` (§4.3). |

Hardware evidence that no auth exists:

- Buds 3 answered every read with only the `0x0100` handshake (OB PhaseB §6: "Auth: none required").
- OPPO ETI81 did reads **and writes** with only `0x0100` (MC `handshake.md`).
- Buds 4 and Pro 3: QB's init (§3.2) never sends HELLO/REGISTER, and set commands work (QB MODELS.md: Pro 3 owner-confirmed ANC/EQ/etc.).
- NP/ND (Pro 3) "work" because they send a valid `0x0100` (+ an unrequested `0x8500`, which is harmless).

### 3.2 Init sequence (HW, OnePlus Buds 4, matches HeyMelody: QB §6, `desktop-tauri/src/session.rs::init`)

Wait ~300 ms after the socket opens, then send one message every ~200 ms. Always answer buds requests (§3.3) while doing this.

1. `0x0100` handshake → `0x8100` = `00 <capability bitmap, LSB-first>` (§3.4).
   Buds 4: `00 FF 77 5A EA 67 0E 20 07` (HW QB §5). Buds 3: `00 FF 77 5A EE E7 0E 22 00` (HW OB PhaseB §6).
2. `0x0103` product id → `00 <3 bytes LE>` (§6).
3. `0x0200` offered events → `00 <count> <event ids>`.
   - Buds 4 / Buds 3: `00 09 01 02 03 04 08 0B F1 F2 F3` (HW QB §6, OB C6).
   - Nord Buds 3 Pro: `00 02 02 01` (HW QB §6).
4. Subscribe:
   - If the bitmap has `0x0205` (bit 22): send `0x0205 <count> <ids…>`. **The count comes first.**
   - Otherwise: send `0x0201 <id>` once per id (§4.2).
5. Reads:
   - `0x010D <count> <feature ids>` (seq 00)
   - `0x010C 01 01` (ANC mode)
   - `0x010C 04 01` (the level Smart chose; models with Smart)
   - `0x0130`, `0x0106` (seq F0), `0x0109` (seq F2), `0x0108`, `0x010C 02 01`, `0x0105`
   - `0x010F`, `0x0122`, `0x0124`
   - `0x012A`, `0x0123`, `0x0114`, `0x012B`, `0x0134`
6. If the bitmap has bit 56 (`0x012F`), the reads after the subscribe can go in **one batch frame** (§4.13), as HeyMelody does. Otherwise send them one by one.
7. After init **do not poll**: battery, wear, ANC and game mode arrive as pushes (HW QB §6).
   - Optional backstop: battery poll every 5 min (QB Android) or 60 s (GZ via MC).
   - Pushes are edge-triggered with no heartbeat (MC notifications.md).

Gadgetbridge does the same thing more simply (`OppoHeadphonesSupport.initializeDevice`): battery, `0x010D`, `0x0109`, `0x010C`, `0x0108 02 03 01`, `0x0205`, `0x0105`. It sends one command at a time, waits ≤ 2 s for the reply, and retries 3× on timeout. It sends **no** `0x0100`.

### 3.3 Requests from the buds that the host must answer (QB §11, `core/src/msg.rs::answer`)

The answer always uses cmd = request | `0x8000` and the request's seq.

| Buds send | Host answers with payload | Notes |
|---|---|---|
| `0x0500` (time request), payload empty or `00 00 00 00` | `00 <unix seconds u32 LE>` | **HW** Nord Buds 3 Pro (`064414`, QB issue #2): **if it gets no answer, it re-sends, then stops answering queries, and resets the link 3-5 s after connect.** Buds 4 sends it but works either way (answered within 5 ms). Buds 3 did not send it (OB C4). **VENDOR**: HeyMelody appends `<tz offset seconds s32 LE>` for models with whitelist flag `utcTimeZone`. **The Buds Pro 4 (`067814`) entry has `utcTimeZone: 1`** (MC `whitelist.json`), so answer it with 8 bytes: `00 <u32 time> <s32 tz>`. Not hardware-verified for Pro 4. |
| `0x050C` | `00 01 00` | foreground app not on list |
| `0x051C` | `00 00 00` | app capabilities |
| `0x051D` | `01` | phone spatial mode unknown |
| push `0x0204` with event `F2` | `0x8204` `00 F2` | the buds wait for it (QB `msg.rs`) |
| push `0x0204` with event `F4` (JSON) | `0x8204` `00 F4 {"cmd":<its cmd>}` (`01 F4` if not JSON, `00 F4` if empty) | |
| any other `0x05xx` request | `01` | "unknown" |

Do **not** answer battery, wear, ANC, `06` pushes, or `0x0501`, `0x0502`, `0x0504`.

Example time answer (buds sent `0x0500` with seq `41`): `AA 0C 00 00 00 85 41 05 00 00 B5 50 A0 69` (this is exactly the draft's "REGISTER", time 2026-02-26). With a +3 h tz for Pro 4: `AA 10 00 00 00 85 41 09 00 00 B5 50 A0 69 30 2A 00 00`.

### 3.4 Capability bitmap (`0x8100`)

- Payload: `00` + bitmap bytes. Bit *n* is LSB-first within each byte (bit 0 = byte0 & 0x01).
- Each bit unlocks a group of command ids. HeyMelody never sends a command whose bit is clear.
- Always enabled: `0x0100`-`0x0104`, `0x0106`, `0x010B`, `0x010D`, `0x0F00`, `0x0F03`, `0x0F04`, and the `0x02xx` family.
- Full 69-row table: QB §5 = MC `handshake.md`. Key rows:

| Bit | Unlocks | Bit | Unlocks |
|---|---|---|---|
| 0 | `0105` fw | 22 | `0205` bulk subscribe |
| 1 | `0106` battery | 26 | `011A` `0412` personalised ANC |
| 3 | `0108` `0401` `0416` gestures | 29 | `0112` `040B` multipoint list / op |
| 4 | `0109` wear | 34 | `0122` `0418` custom EQ |
| 5 | `0400` find buds | 37 | `0123` `041A` codec list / set |
| 7 | `0403` feature switch | 38 | `0124` `041B` BassWave level |
| 8 | `010C` `0404` ANC | 43 | `EF03` `041E` buds render 3D audio |
| 9 | `0405` fit test | 47 | `0422` `012A` spatial type |
| 10 | `0406` `010F` EQ preset | 49 | `0423` `012B` game sound |
| 19 | `040E` `040D` `0115` `0116` Golden Sound | 56 | `012F` batch query |
| 21 | `0410` `0119` | 57 | `0427` `0130` prompt volume |
| | | 59 | `0429` `0132` device manager |
| | | 61 | `042D` `0133` tap sensitivity |
| | | 66 | `0431` `0134` head gestures |

**Gate every feature on the bitmap.** A wrong command number fails **silently**: no ack, no error (HW QB §15: `0x0402`).

---

## 4. Commands

`<seq>` below is `11` etc. as a placeholder; use any value except `FF`. All example frames were generated with the §1.1 encoder and cross-checked against captured frames where marked.

### 4.0 Ack and status semantics

- A write reply is `0x84xx` with `payload[0]` = status (QB §3, §16):

| Status | Meaning |
|---|---|
| `00` | **accepted** (not necessarily applied) |
| `01` | command or feature id not supported / failed |
| `02` | refused now (condition; e.g. Hi-Res `18 01` while the phone plays AAC) |
| `0E` | ANC set refused: **no bud in an ear** |
| `15` | personalised ANC: another device is busy |

- **An ack is never proof.** Read the state back, or wait for the push.
  - Buds 4 acked `00` and silently dropped bits (QB §14).
  - GZ saw `0E` on Pro 2 for an already-active mode, and for Transparency while charging. Some sessions sent no ack at all.
  - Correct pattern (GZ, QB): send once, never replay the write, then re-query (e.g. `0x010C 01 01` after ~1 s, repeating if stale).
- Query replies: `0x81xx` with `payload[0]` = status `00`, then the data.

### 4.1 Battery: query `0x0106`, reply `0x8106`, push event `01`

- Request: `AA 07 00 00 06 01 F0 00 00` (QB uses fixed seq `F0`; NP/OOB captured `AA 07 00 00 06 01 25 00 00`).
- Reply: `00 <count> (<part> <raw>)×count`. Push: `0x0204` with `01 <count> (<part> <raw>)×count`.
- `part`: `01` left, `02` right, `03` case. Level = `raw & 0x7F` (%). Charging = `raw & 0x80`. **HW** (QB §13, MC battery.md, GB `parseBattery`, ND `DecodeBattery`).
- Level `0` = unknown; hide it. GB skips part `0xFF`.
- **The count varies.** The case entry is dropped when both buds are out, or when the lid is closed (only the bud outside is reported, e.g. `01 01 5A`). Key the entries by part id, never by position (HW QB, MC).
- Example reply: `AA 0F 00 00 06 81 F0 08 00 00 03 01 64 02 E4 03 50` → L 100 %, R 100 % charging, case 80 %.
- Example push: `AA 0F 00 00 04 02 FF 08 00 01 03 01 5A 02 5A 03 E4` → L 90, R 90, case 100 % charging.
- Pushes arrive every 1-6 min on the buds' own clock, plus on every change (QB §13).

### 4.2 Events: `0x0200` list, `0x0205` / `0x0201` subscribe, `0x0204` push

- `0x0200` (empty): `AA 07 00 00 00 02 <seq> 00 00`. The reply `0x8200` is `00 <count> <ids>`.
- **Bulk subscribe** `0x0205 <count> <ids>` (bitmap bit 22):
  - `01 02 03`: `AA 0B 00 00 05 02 <seq> 04 00 03 01 02 03`.
  - Buds 3/4 full set `01 02 03 04 08 0B`: `AA 0E 00 00 05 02 <seq> 07 00 06 01 02 03 04 08 0B` (**HW** answered on Buds 3, OB C3).
  - **Never** send `01 01 02 02`: that means "count 1, battery only". The buds accept it and never push wear (HW QB §6).
  - Only subscribe ids the buds offered in `0x8200`.
  - Ack `0x8205`: `01 <count> (<id> 00)…`. Its meaning is a GUESS, and nobody relies on it.
  - Nord Buds 3 Pro sent **no** `0x8205` (HW QB §6).
  - GB sends `09` as the first byte (not the count) followed by 2-5 ids. That works on its realme/OPPO targets but contradicts QB's hardware finding. Use count-first.
- **Per-event** `0x0201 <id>` (when bit 22 is clear): `AA 08 00 00 01 02 <seq> 01 00 02`. The reply `0x8201` is `<status> <id>`. ETI81 ignored `0x0205` with no reply (MC).
- **Push** `0x0204`, seq `FF`, `payload[0]` = event id (QB §10, MC notifications.md):

| id | event | body after the id |
|---|---|---|
| `01` | battery | `<count> (<part> <raw>)…` (§4.1) |
| `02` | wear / case | `<count> (<part> <state>)…` (§4.4) |
| `03` | ANC | `01 01 <mode mask LE 1-4 B>` = current mode; `02 <kind> <mask>` = hold-cycle mask (**not** a mode); `04 01 <mask>` = the level Smart picked |
| `04` | fit test result | `<dev> <res>`… (`1` good, `0` average, `6` poor) |
| `05` | game mode | `<state>`; on `062410 062810 063010 063410` it is `<x> <x> <state>` |
| `06` | dual-connection device list | `<count>` + entries (§4.10) |
| `08` | hearing test status | `<kind> <status>` |
| `0B` | personalised ANC result | `<r>` |
| `0E` | ear-scan result | |
| `0F` | public event | `<flags u16 LE> <data>`: bit0 prompt volume, bit4 tap level |
| `F1` | gesture fired (diagnostic only) | `<side> <button> <act> <fn> <context> [opts]` |
| `F2` | connected devices | **answer `0x8204 00 F2`** |
| `F4` | diagnostic JSON | answer (§3.3) |

  - Event `03` does not arrive unless you subscribed to `03` (HW QB §14). Nord Buds 3 Pro pushed it once without the subscription.
  - Other push commands: `0x0503 <count> (<id> <val>)…` (feature switch changed, §4.5), `0x0504 <id>` (EQ changed), `0x0506` (custom EQ list), `0x0508 FB 05 <level> <band>`, `0x0510 <type>` (spatial type) (HW QB §10).

### 4.3 ANC / noise control: query `0x010C`, set `0x0404`, push `03`

**Read** (`0x010C <what> <arg>`):

| Request payload | Example | Reply payload `0x810C` |
|---|---|---|
| `01 01` current mode | `AA 09 00 00 0C 01 <seq> 02 00 01 01` | `00 01 01 <report mask LE>` |
| `04 01` level Smart chose | `AA 09 00 00 0C 01 <seq> 02 00 04 01` | `00 04 01 <SET-bit mask LE>` |
| `02 01` hold-cycle members (`02 03` left / `02 04` right on per-bud models) | `AA 09 00 00 0C 01 <seq> 02 00 02 01` | `00 02 01 <SET-bit mask LE>` |

Sources: HW QB §14, ND `GET_ANC_NOW`/`GET_ANC_AUTO`, GB `AncConfigType` (`01` MODE, `02` TOUCH_CYCLE_MODES, `04` DYNAMIC_LEVEL).

Example reply: `AA 0C 00 00 0C 81 <seq> 05 00 00 01 01 08 00` = Off.

**Set** `0x0404 01 01 <mask>`:

- The mask is `1 << bit` and takes `bit/8 + 1` bytes, LE.
- **The bit is the mode's `protocolIndex` from the model's HeyMelody tree, not a fixed enum.**
- **SET bits ≠ REPORT bits.** A report names the *child* of the mode that was set.

OnePlus "Buds-4-style" tree. It is the same for **Buds Pro 4 `067814`, Buds Pro 3 `064014`, Buds 3 `063C14`, Nord Buds 3 Pro `064414`, Buds 4 `065414`** (MC `profiles.json`/`whitelist.json` `noiseReductionMode`; QB `models.json` `ancModes`):

| Mode | SET bit | SET payload | Frame (`<seq>` = 11) | REPORT value (LE u16) |
|---|---|---|---|---|
| Off | 0 | `01 01 01` | `AA 0A 00 00 04 04 11 03 00 01 01 01` | `0x0008` (child bit 3) |
| ANC (last used level) | 1 | `01 01 02` | `AA 0A 00 00 04 04 11 03 00 01 01 02` | the active level's bit (`0x0010`…`0x0080`), or `0x0002` |
| Transparency | 2 | `01 01 04` | `AA 0A 00 00 04 04 11 03 00 01 01 04` | `0x0100` (child bit 8); `0x0200` = transparency + voice enhance |
| ANC Deep / "High" | 4 | `01 01 10` | `AA 0A 00 00 04 04 11 03 00 01 01 10` | `0x0010` |
| ANC Medium / "Moderate" | 5 | `01 01 20` | `AA 0A 00 00 04 04 11 03 00 01 01 20` | `0x0020` |
| ANC Light / "Low" | 6 | `01 01 40` | `AA 0A 00 00 04 04 11 03 00 01 01 40` | `0x0040` |
| ANC Smart / "Auto" (firmware picks the level) | 7 | `01 01 80` | `AA 0A 00 00 04 04 11 03 00 01 01 80` | `0x0080`; then push `03 04 01 <mask>` gives the picked level (e.g. `20 00` = Medium) |
| **Adaptive** (Buds 4 and **Buds Pro 4 only**) | **11** | `01 01 00 08` | `AA 0B 00 00 04 04 11 04 00 01 01 00 08` | `0x0800` |
| Pro 4 Adaptive children (modeType 11/12/13) | 12 / 13 / 14 | `01 01 00 10` / `00 20` / `00 40` | `AA 0B 00 00 04 04 11 04 00 01 01 00 10`… | `0x1000`/`0x2000`/`0x4000` |

Evidence for this table:

- **HW**: Buds 4 (QB §14 `[CAPTURE]`).
- **HW**: Buds 3, every set mask read back as expected, fw 127.127.101 (OB §9 C2).
- **HW**: Buds Pro 3. NP/ND use exactly `AA 0A 00 00 04 04 42 03 00 01 01 {01 Off, 02 ANC, 04 Transparency}` and 2-byte `AA 0B … 01 01 {10 High, 20 Moderate, 40 Low, 80 Auto} 00`, and decode reports `0x0008` Off, `0x0100` Transparency, `0x0010..0x0080` levels. The QB owner confirmed noise control.
- **HW**: Nord Buds 3 Pro. OOB `ANCWireMapping.swift`: `0x02` ANC, `0x04` Transparency, `0x01` Off "(observed)"; QB owner-confirmed.
- **Bit 8 is NOT Adaptive.** `01 01 00 01` sets plain Transparency (QB §14, from the firmware image).
- **Pro 4's Adaptive children (bits 12-14, HeyMelody modeType 11/12/13) are VENDOR-only.** No source names them or has tested them (QB `AncModes.kt` has no name for modeType 11-13). Send bit 11 for Adaptive, and only decode 12-14 from reports.
- QB `[FIRMWARE]`: Buds 4 firmware treats bits `0x001` and `0x008` both as Off. Pro 3 and Nord Buds 4 Pro have no Adaptive.

Other trees, so the code must be **table-driven per product id**:

- realme / Gadgetbridge (`AncConfigValue`): Off `01`, Transparency `02`, ANC `08`; levels Low `04`, High `08`, Normal `10`, Dynamic `20`. Set value = report value.
- Original OnePlus Buds Pro `060C14`: Off `01`, Transparency `02`, light `04`, deep `08`, smart `10` (HW GZ).
- 9 models use `NC 0, Off 1, Transparency 2`, where the Buds 4 "Off" bytes turn ANC **on** (QB §21.3, e.g. Nord Buds 3 `065014`).

Behaviour:

- **Wear rule.** With **no** bud in an ear the buds answer `0x8404` status **`0E`** and do nothing. **One bud is enough** (HW QB §14, Buds 4). After a `0E`, re-read `0x010C 01 01`.
- A `0x0404` ack comes first. The push `03 01 01 <mask>` follows ~1-2 s later when the change is applied (or when it was made by a bud gesture).
- **Hold-cycle membership** (which modes the long-press cycles through) is a different payload on the same command: `0x0404 02 <kind> <SET-bit mask LE, 1-4 B>`. `kind` is `01` shared, `03` left, `04` right.
  - **HW** Buds 4 `[CAPTURE]` of HeyMelody: `AA 0A 00 00 04 04 <seq> 04 00 02 01 07 08` added Adaptive (mask `0x0807`).
  - Buds Pro 4 has whitelist `longPressType` set, which suggests per-bud kinds `03`/`04` (**VENDOR**).
  - GB requires ≥ 2 modes in the cycle.
- Push parsing: accept a mode only when the body after `03` starts with `01 01`. `03 02 …` is a cycle mask (HW bug in QB, §14).
- Captured push: `AA 0C 00 00 04 02 FF 05 00 03 01 01 08 00` = Off (QB §2). `… 03 01 01 20 00` = Medium. `… 03 01 01 00 01` = Transparency (QB `Anc.kt` header).

### 4.4 Wear / in-case status: query `0x0109`, reply `0x8109`, push event `02`

- Request: `AA 07 00 00 09 01 F2 00 00` (QB fixed seq `F2`).
- Reply: `[00] <count> (<part> <state>)…`. Some replies omit the status byte, so accept both offsets (HW QB §13).
- State bits (**VENDOR** `StatusInfo`, HW-consistent):

| Bit | Meaning |
|---|---|
| `0x01` | out of the case |
| `0x02` | **in ear** |
| `0x04` | lid open (case entry and buds in the case) |
| `0x08` | safe-remind (clear it) |

  - Common values: `0` = in case with the lid closed, `4` = in case with the lid open, `1`/`5` = out but not worn, `3`/`7` = worn. (GB `EarbudsStatusValue`: `00` IN_CASE, `01` OFF_EAR, `03` READY.)
  - ND `DecodeWear`: "bit 0x01 clear = in case; bit 0x02 set = in an ear".
- Example push: `AA 0F 00 00 04 02 FF 08 00 02 03 01 03 02 03 03 04` → L worn, R worn, case lid open.
- Some models have no in-ear sensor and report `1` always: realme T200, T200x, Nord Buds 3r (QB §13).
- Component order is not stable; key by id (MC).
- Auto-pause on wear is a **feature switch** `04` (§4.5), not this query.

### 4.5 Feature switches: write `0x0403 <id> <01|00>`, read `0x010D <count> <ids>`, push `0x0503`

- Write: `AA 09 00 00 03 04 <seq> 02 00 <id> <01|00>`. The reply `0x8403` is `<status>` (`00` ok, `01` no such id, `02` refused now).
- Read: e.g. the Buds 4 list (QB `[CAPTURE]`): `AA 11 00 00 0D 01 00 0A 00 09 05 04 0B 11 18 06 1B 1D 1C`.
  - The reply `0x810D` is `00 <count> (<id> <value>)…`.
  - Ids the buds lack are **omitted**. Buds 4 answered `00 08 05 00 04 00 0B 01 11 01 18 01 06 00 1B 00 1D 01` (no `1C`).
- **Do not send a long list to models that cannot take it.** A 23-id list dropped the Enco Buds2 link (QB issue #5). Send the per-model `statusQuery`:

| Model | statusQuery ids (QB `models.json`, from HeyMelody) |
|---|---|
| Buds Pro 4 `067814` | `05 04 0B 0F 11 13 18 06 1B 1D 1C 1E 27 28 32 37 3B 3A` |
| Buds Pro 3 `064014` | `05 04 0B 11 13 18 06 1D` |
| Buds 3 `063C14` | `05 04 0B 11 18 06 1B 1D` |
| Nord Buds 3 Pro `064414` | `05 04 0B 0C 11 18 06 1B 1D` |
| Buds 4 `065414` | `05 04 0B 11 18 06 1B 1D 1C` |

Feature ids (QB §16; GB `MiscConfigType`; BL `opoBudsConfig.js`; NP):

| Id | Feature | Frame "on" | Notes |
|---|---|---|---|
| `04` | wear detection (auto play/pause) | `AA 09 00 00 03 04 <seq> 02 00 04 01` | HW |
| `06` | **game mode / low latency** | `… 02 00 06 01` | HW on many models. Always accepted, applied after 500 ms; the confirmation is push `05` ~1 s later (QB §16). On models whose bitmap has `0x0423` the main switch is `28`. **Pro 3 acks `28 01` but reads back `0`** (HW QB §21.5): use `06` there. |
| `0B` | Golden Sound / hearing profile | | HW |
| `11` | **dual connection (multipoint)** | `… 02 00 11 01` | HW. HeyMelody follows it with `0x0413 08 00 00` (on) / `08 00 01` (off) (QB §19). |
| `18` | High-quality audio (Hi-Res; LHDC/LDAC bitrate) | | Status `02` unless the **phone** link plays LHDC/LDAC, so **a PC is always refused** (QB §16). An accepted write restarts the link ~4 s later. |
| `1B` | spatial / 3D audio (models **without** `0x012A`) | `… 02 00 1B 01` | Mutually exclusive with `18`. HeyMelody sends `1B 01` then `18 00`. |
| `1C` | smart volume | | VENDOR |
| `1D` | **BassWave on/off** | `… 02 00 1D 01` | HW (NP `basswave_toggle`, QB) |
| `17` | power saving | | **Restarts the buds.** Do not expose it. |
| `05` | unknown | | HeyMelody always asks for it first and never writes it |

Firmware tables of which ids each model actually stores are in QB §16 "What the firmware accepts". For example, Pro 3: `02 04 05 06 0B 0C 0D 11 12 13 14 15 16 18 19 1D 22 23 24 27 28`, **with no `1B`/`1C`**.

### 4.6 EQ presets: select `0x0406 <id>`, read `0x010F`, push `0x0504 <id>`

- Select: `AA 08 00 00 06 04 <seq> 01 00 <id>`. The reply `0x8406` is `00`.
- Read: `AA 07 00 00 0F 01 <seq> 00 00`. The reply `0x810F` is `00 <id>`. Ids ≥ 4 are custom presets on Buds 4.
- **Preset ids are per model** (`eqPresets.slot` = `0x0406` id). Sources: MC `profiles.json` `eq.modes`, QB `models.json`; names from QB `EqActivity.kt`; Pro 3 names from NP:

| Model | id → preset |
|---|---|
| **Buds Pro 4 `067814`** | 0 Balanced · 1 Bold · 2 Clear Vocals ("Serenade") · 3 Bass · 7 Dynaudio featured (VENDOR) |
| **Buds Pro 3 `064014`** | 0 Balanced · 1 Bold · 2 Serenade · 3 Bass · **7 Dynaudio** (HW: NP `eq_preset_packet` 0,1,2,3,7; QB owner-confirmed EQ) |
| **Buds 3 `063C14`** | 0 Balanced · 1 Bass · 2 Clear Vocals · 3 Bold (only fw ≥ 119) (VENDOR; OB lists the same ids as Balanced / Deep Sea Bass / Pure Vocals / Bright & Crisp) |
| **Nord Buds 3 Pro `064414`** | **3** Balanced · **4** Clear Vocals · **5** Bass (VENDOR; note the ids start at 3) |
| Buds 4 `065414` | 0 Balanced · 1 Clear Vocals · 2 Bass (HW QB) |

### 4.7 Custom EQ: list `0x0122`, write `0x0418`

- List: `AA 07 00 00 22 01 <seq> 00 00`. The reply `0x8122` is `00 <count>`, then per preset `<selected> <min> <max> <id> <nameLen> <name UTF-8> <bandCount> (<freq u16 LE> <gain s8>)×bandCount`. The tag is `FA 06` = −6..+6 dB (QB `Eq.kt EqCodec.parseList`).
- Write: `0x0418 <action> <min=FA> <max=06> <id> <nameLen> <name> <bandCount> (<freq u16 LE> <gain s8>)…`. Actions:
  - `01` create (send id `00`; the buds assign the id and return it in `0x8418 00 <id>`);
  - `02` save **and select** (send the whole preset);
  - `03` delete (the selection falls back to `00`).
- Ids are renumbered after create and delete, so re-read `0x0122`. Max ~3 custom presets (QB §17; NP `custom_eq_packet` matches byte-for-byte).
- Bands:
  - **Buds Pro 4: 10 bands** `31 62 125 250 500 1k 2k 4k 8k 16k` (whitelist `customEqFrequency`, VENDOR).
  - Pro 3 / Buds 3 / Nord Buds 3 Pro / Buds 4: **6 bands** `62 250 1k 4k 8k 16k` (HW NP Pro 3).
- Example, create "Rock" +4 +2 0 +1 +3 +2:
  `AA 23 00 00 18 04 <seq> 1C 00 01 FA 06 00 04 52 6F 63 6B 06 3E 00 04 FA 00 02 E8 03 00 A0 0F 01 40 1F 03 80 3E 02`

### 4.8 BassWave level: `0x041B FB 05 <level s8>`, read `0x0124`

- On/off is feature `1D` (§4.5). Level range is −5..+5; `FB 05` is the (min, max) pair HeyMelody sends.
- Examples: +3 = `AA 0A 00 00 1B 04 <seq> 03 00 FB 05 03`, −2 = `… FB 05 FE`. The reply `0x841B` is `00`.
- Read: `AA 07 00 00 24 01 <seq> 00 00` → `0x8124 00 FB 05 <level>`.
- Sources: HW NP/ND `basswave_level`; QB §17.
- realme dynamic-sound models append a band byte and use `0x012C` instead.

### 4.9 Spatial / 3D audio

- If the bitmap has bit 47 (`0x0422`/`0x012A`):
  - Write `0x0422 <type>`: `0` off, `1` fixed, `2` head-tracking. Example: `AA 08 00 00 22 04 <seq> 01 00 02`.
  - Read `0x012A` → `00 <type>`. Push `0x0510 <type>`.
  - Head tracking exists only where `spatialTypes` contains 2: **Buds Pro 3, Buds Pro 4**, Buds Pro 2, Enco X3 (MC whitelist).
  - Status: **VENDOR**, "wired, unverified" (QB §18). The QB Pro 3 owner confirmed "3D audio" working.
- Otherwise use feature `1B` (§4.5). Buds 3 / Nord Buds 3 Pro / Buds 4 have `spatialTypes [0,1]`.
- `0x041E 00` = buds render 3D (bitmap bit 43 with `0xEF03`).
- The 3D effect may need an OPPO/OnePlus phone. Nord Buds 3r accepts `1B` but plays no effect on other hosts (HW QB §18). **Expect spatial to be meaningless when the PC is the audio source** (GUESS, consistent with QB hiding the row on desktop).

### 4.10 Dual connection

- Toggle: feature `11` (§4.5), then `0x0413 08 00 <00 on | 01 off>`: `AA 0A 00 00 13 04 <seq> 03 00 08 00 00` (HW QB §19, Buds 4).
- Device list: `0x0112` → `0x8112 00 <count>`, then per entry `<MAC 6, reversed> <len> <state 02=connected/00> <flags> <nameLen> <name>`. It is also pushed as event `06` (HW QB §19).
  - Flags: bit0 = this host, bit1 = main audio device, bit2 = playing.
- Connect / disconnect another device:
  - `0x0429 01|02 <MAC, written order>`, preferred device `0x0429 04 00|01 <MAC>` (bitmap bit 59, VENDOR).
  - Or the older `0x040B` (bitmap bit 29; GB `MULTIPOINT_DEVICES_SET`, BL `OPERATE_MULTI_CONNECT`).
  - Pick by bitmap.
- Pro 4 whitelist: `multiDevicesConnect: 2` (vs 1 on the others). Its meaning is unknown (VENDOR).

### 4.11 Find my buds `0x0400`

- Start `AA 08 00 00 00 04 <seq> 01 00 01`, stop `AA 08 00 00 00 04 <seq> 01 00 00`. Both buds ring; there is no side byte. The reply is `0x8400 00`.
- Sources: HW QB §18; GB `encodeFindDevice`. Warn the user if the buds are in their ears.

### 4.12 Firmware / identity

- `0x0105` → `0x8105 00 <count> <ASCII "part,type,value" triples joined by ','>`.
  - Example, Buds 3: `"1,1,01,1,2,127,2,1,01,2,2,127,3,2,101"`.
  - Display the type-2 values for L.R.case = `127.127.101` (HW OB, QB, GB `FIRMWARE_RET`). Type `1` = hardware, `2` = running fw, `3` = backup, `4` = vendor (MC version.md).
  - realme already returns dotted versions (GB).
- `0x0103` product id (§6). `0x010B` colour id. `0x0117 03 02 03 04` serial numbers (VENDOR).

### 4.13 Gestures: read `0x0108`, write `0x0401`

- Read: `AA 07 00 00 08 01 <seq> 00 00`. The reply `0x8108` is `00 <count> (<dev> <button> <act> <fn>)…`.
  - GB sends payload `02 03 01` with the read. QB sends it empty. Both work.
- Write: `0x0401 <count> (<dev> <button> <act> <fn>)…`. **No status byte in the write.** The reply `0x8401` is `00`.
  - Captured ack: `AA 08 00 00 01 84 .. 01 00 00` (QB `Protocol.kt`).
  - Example (left, button 1, double, game mode): `AA 0C 00 00 01 04 <seq> 05 00 01 01 01 02 11`.
- `dev`: `01` L, `02` R, `04` both.
- `button`: `01` touch/pinch, `06` on-call group.
- `act`: `01` single, `02` double, `03` triple, `04` hold, `05` slide, `06` long hold.
  - Pinch models (Pro 3 / Pro 4) list whitelist actions 16/17/18/20, which map to wire acts 1/2/3/4 (QB §21.4).
- `fn` (HW Buds 4, QB §15):

| `fn` | Function | `fn` | Function |
|---|---|---|---|
| `00` | none | `0B` / `0C` | vol up / down |
| `01` | play/pause | `0D` | switch device |
| `03` | voice assistant | `11` | game mode |
| `05` / `06` | prev / next | `12` | zen |
| `07` | volume slide | `19` | AI summary |
| `08` | ANC cycle | `1C` / `1D` | call decline / answer-end |
| `0A` | track slide | | |

- **Always read the table back** after a write. `0x0402` and wrong ids are silently ignored (HW QB §15).
- Push `F1` (gesture fired) is diagnostic only. Example: `AA 0D 00 00 04 02 FF 06 00 F1 02 01 04 08 03`.

### 4.14 Other useful commands (VENDOR unless marked)

- Prompt volume: `0x0130` → `00 <level>`; write `0x0427 <1..10>` (HW). Each write plays a tone.
- Tap sensitivity: `0x0133` / `0x042D <1..5>`.
- Codec: `0x0114` → `00 <codec>`. Codecs: `1` SBC, `2` AAC, `3` LDAC, `7` LHDC, `8` LHDC V5. HW Buds 4 `00 08`.
- Fit test: `0x0405 01|00`; result push `04` (HW Buds 4 ~8.6 s).
- Batch `0x012F` (bit 56): request `<count> (<cmd LE> <len LE> <payload>)…`. Replies are one or more `0x812F 00 <count> (<cmd LE> <len LE> <reply payload>)…`, and each answer is handled as `cmd|0x8000` (HW QB §12).

---

## 5. Model identification

Product id `0x0103`: reply `00 <id 3 bytes LE>`, shown as `%06X`. Example: `00 14 54 06` → `065414` (HW QB, MC, GB).

| Model | Product id | `0x8103` payload | BT name (HeyMelody `name`, matched exactly) | Evidence |
|---|---|---|---|---|
| **OnePlus Buds Pro 4** | **`067814`** | `00 14 78 06` | `OnePlus Buds Pro 4`; whitelist alias `children: [{"name":"OnePlus Buds Bohr"}]` | VENDOR (MC `whitelist.json`; QB `models.json`, not owner-confirmed yet) |
| OnePlus Buds Pro 3 | `064014` | `00 14 40 06` | `OnePlus Buds Pro 3` | VENDOR id; HW use (NP, QB owner) |
| OnePlus Buds 3 | `063C14` | `00 14 3C 06` | `OnePlus Buds 3` | **HW** (OB PhaseB §6: reply `00 14 3C 06`) |
| OnePlus Nord Buds 3 Pro | `064414` | `00 14 44 06` | `OnePlus Nord Buds 3 Pro` | HW (QB issue #2) |
| OnePlus Buds 4 | `065414` | `00 14 54 06` | `OnePlus Buds 4` | HW (QB) |
| OnePlus Buds Pro 2 / Pro | `062014` / `060C14` | | | HW (GZ) |

- Lookup order (VENDOR, QB §6): an exact BT-name match first, then the product id. **The name wins.** OPPO Enco Buds2 reports `060C12`, which is the realme Q2s id. Renamed devices fall back to the id.
- realme T200 add 3 changing bytes after the id; use only the first 3.
- Colour ranges fold: `100100-100102` → `060414`, etc.
- The BLE advertisement carries the same id (§2.2).
- Whitelist flags worth knowing for Pro 4 (MC `whitelist.json`, VENDOR):
  - `utcTimeZone 1` (§3.3), `wearDetection 2`, `multiDevicesConnect 2`, `spatialTypes [0,1,2]`;
  - `highToneQuality`, `bassEngineSupport`, `supportPinch`, `fitDetection`, `heyFindDevice`, `headMotion`, `spineHealth 2`, `zenMode 3`;
  - `customEqFrequency` 10 bands, `minVersion 16010000`.

---

## 6. Quirks and timing (all HW unless marked)

1. **Answer `0x0500`**, or Nord Buds 3 Pro drops the link 3-5 s after connect (QB §11). Implement §3.3 in the read loop from the first byte.
2. **No bud in an ear → ANC set returns `0E`**; one bud is enough (QB §14). The draft's UI rule (disable the ANC selector when both buds are out, from wear state/pushes) is right. Still send the command if the state is unknown, and handle `0E`.
3. **Case lid closed → the case and buds inside stop reporting, and the socket drops ~1-3 s later.** Last pushes: `01 00 02 00 03 00`. Reconnect when the buds reappear (QB §13, MC).
4. An **ack is not success**. Re-read, or wait for the push. Never replay writes blindly (GZ, QB).
5. **Seq**: match on `(seq, cmd&0x7FFF)`. Pushes use `FF`. Batch replies carry the batch's seq.
6. **Pacing**: ~200 ms between init messages (QB). GB serialises with a 2 s timeout and 3 retries.
   - The draft's 1.5-2 s waits come from the bogus HELLO/REGISTER. They are unnecessary.
   - Do one write at a time, and wait for each write to finish.
7. **Multipoint**: the PC and the phone each get their own `079A` link. Some states are **per link**: Hi-Res `18` and codec `0x0114` (QB §18).
   - Hi-Res from a PC is always refused (`02`), because the PC link is AAC/SBC.
   - Spatial audio from a PC likely has no effect.
8. Changes that **restart the buds or the link**: feature `17` (power saving), an accepted `18`, and `0x041A` (codec set). The link returns in ~4-12 s (QB §16, §18).
9. The **subscription is not a state request**. Reconnecting re-sends nothing, so re-subscribe and re-read on every connect (MC).
10. HeyMelody on the same phone also holding the link is fine for a PC client. On a single host, only one process may own the RFCOMM channel.

---

## 7. Recommended minimal implementation for the widget

1. Connect over RFCOMM by UUID `079A` (Windows: SOCKADDR_BTH serviceClassId, port 0). Fallbacks: `1107`, then channel 15.
2. Run the framer (§1.2) and auto-answer `0x05xx` and `F2`/`F4` (§3.3).
3. `0x0100` → bitmap; `0x0103` → product id → load the model table (ANC bits, EQ ids, statusQuery). `0x0200` → offered events; `0x0205 <n> <offered ∩ {01,02,03,04,05,06}>`.
4. Read `0x010D <statusQuery>`, `0x010C 01 01`, `0x010C 04 01` (Smart models), `0x0106`, `0x0109`, `0x010F`, `0x0124`, `0x0105`.
5. Drive the UI from the pushes `0x0204` (01 battery, 02 wear, 03 ANC, 05 game, 06 devices), `0x0503`, `0x0504`, `0x0510`.

---

## 8. What is still unverified for Buds Pro 4 specifically

- Product id `067814` and every Pro-4 table come from the HeyMelody whitelist. **No published Pro 4 capture exists** in any project read; QB lists Pro 4 as "detected, not confirmed".
- Things to capture on the user's Pro 4 first:
  - the `0x8100` bitmap;
  - the `0x8200` list;
  - whether it sends `0x0500` and needs the tz-offset form;
  - whether `0x0422` or `1B` drives spatial;
  - the Adaptive children bits 12-14;
  - the per-bud hold-cycle kinds `03`/`04`.
- Pro 4 has no firmware image analysed. The Pro 3 firmware facts (QB §16) are the nearest relative.

---

## 9. Corrections to draft §2.2 (`claude_code_earbuds_widget_prompt.md`)

| Draft claim | Reality | Source |
|---|---|---|
| HELLO `AA 07 00 00 00 01 23 00 00 12` | `0x0100` handshake = `AA 07 00 00 00 01 <seq> 00 00` (9 bytes). The trailing `12` is a stray byte outside the frame. It returns the capability bitmap; it is not "hello". | §3.1; QB §2; OB §5 |
| REGISTER `AA 0C 00 00 00 85 41 05 00 00 B5 50 A0 69` = session token | It is `0x8500`, the **answer to the buds' time request**, carrying a stale Unix time 2026-02-26 13:55:01. There is **no auth**. Send a real `00 <now u32 LE>` (+ `<tz s32>` on Pro 4) **only when the buds send `0x0500`**. | §3.1, §3.3; OB §5; OB/MC/QB HW |
| `LEN` = bytes from byte 4 = total−4 | `LEN` = bytes after itself = **total−2** = `7 + PayLen`, LEB128 (multi-byte above 127). | §1.1; QB, GB, GZ |
| `[PAD 00 00]` | Byte 2 = slice flag (`00` whole, `01-03` slices; realme may send `04`), byte 3 = `00`. | §1.1; QB, GB |
| CMD_LO = "category", CMD_HI = direction (`01` req, `81` reply, `04` set, `84` ack, `02`/`05` push) | Cmd is a u16 LE. The high byte is the class (`01` query, `04` write, `02` events, `05` **requests from the buds that need answers**, not pushes). Reply = cmd \| `0x8000`. The push is `0x0204` with seq `FF`. | §1.1; QB §4 |
| SEQ "+2 on BLE, +1 on RFCOMM" | Arbitrary; echoed in replies; `FF` = push. | §1.1 |
| ANC set modes `01`=ANC, `02`=Transparency, `04`=Off | **`01` = Off, `02` = ANC, `04` = Transparency** on Buds 3/4, Pro 3, Nord Buds 3 Pro. Levels `10` Deep, `20` Medium, `40` Light, `80` Smart (2-byte form `xx 00` also accepted). Adaptive = bit 11 `00 08` (Buds 4, Pro 4). Reports use different bits (`0x0008` Off, `0x0100` Transparency, …). The blog's table was mislabelled. | §4.3; NP/ND, OOB, OB, QB HW |
| GET ANC: `0C 01` on RFCOMM / `04 82` on BLE, `AA 09 00 00 04 82 44 02 00 00 F2` | Same on both transports: `0x010C 01 01` = `AA 09 00 00 0C 01 <seq> 02 00 01 01`. The quoted frame is a `0x8204 00 F2` ack to an `F2` push. | §3.1, §4.3 |
| BLE: `0100079A` write w/o response, `0200079A` notify | Correct (OB observed the properties). But BLE is a fallback; RFCOMM is primary everywhere. | §2 |
| `FE2C1234…/FE2C123A` = "secondary telemetry service"; MUST subscribe before HELLO | FE2C1233-1238 = **Google Fast Pair**. There is no evidence a subscription is needed, and `FE2C123A` does not exist on Buds 3. Only the CCCD on `0200079A` matters. | §2.2; OB C7 |
| RFCOMM channel 15 | True for Buds 3, Buds 4 and Pro 3 (079A on ch 15), but OPPO uses 12. Resolve via SDP; on Windows, connect by service UUID with port 0. | §2.1 |
| `1107` = legacy OnePlus Buds Pro service | Correct (GZ HW). | |
| Push `04 02` = live ANC/wear/battery | Correct: wire bytes `04 02` = cmd `0x0204`. But it **requires `0x0205` subscription with count first** (`03 01 02 03`), and ANC needs event `03`. | §4.2 |
| Wear query `09 01`, ANC refused `0x0E` when not worn | Correct. Clarification: refused only when **no** bud is in an ear; one is enough. | §4.3, §4.4 |
| ANC strength "High/Moderate/Low/Smart" | Correct names. Bits 4/5/6/7. Pro 4 also has Adaptive (bit 11) with 3 unnamed sub-modes (bits 12-14). | §4.3 |
| Buds 3 `063C14`, Nord Buds 3 Pro `064414` | Correct. Pro 4 = `067814`, Pro 3 = `064014`. | §5 |
