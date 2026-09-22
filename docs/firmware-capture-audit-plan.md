# Ledger Firmware Update Capture and Audit Plan

## Purpose

This document is a portable implementation plan for collecting and analyzing
the externally observable artifacts produced during a Ledger firmware update.
It is based on the firmware update implementation in the Bacca
`ledger_installer` repository.

The intended use is authorized security research with spare, seedless devices.
The project should capture Ledger Manager API responses, Ledger Script Runner
WebSocket messages, and USB APDU exchanges without modifying update traffic.

Capturing the complete update protocol does not guarantee access to plaintext
Ledger OS firmware. If decryption occurs inside the OSU or Secure Element, the
host will only observe an encrypted or signed installation stream.

## Scope and Safety Boundaries

- Use only devices you own or are explicitly authorized to test.
- Use factory-reset devices with no valuable seed, accounts, or funds.
- Keep Ledger Live and other Ledger clients closed during a capture.
- Connect only one Ledger device at a time.
- Do not modify, reorder, delay, replay, or inject APDUs during initial work.
- Do not disable TLS verification globally.
- Do not attempt to bypass secure boot, signature checks, or debug protection.
- Keep captures private until licensing and disclosure requirements are known.
- Expect interrupted firmware operations to require official device recovery.

## Source Repository Findings

### Protocol Constants

The source constants are defined in `ledger_manager/src/constants.rs`:

| Setting | Repository value |
| --- | --- |
| Ledger Live common version | `34.0.0` |
| Provider | `1` |
| Manager API v1 | `https://manager.api.live.ledger.com/api` |
| Manager API v2 | `https://manager.api.live.ledger.com/api/v2` |
| Script Runner base | `wss://scriptrunner.api.live.ledger.com/update` |

These values are reverse-engineered integration details and may change. Every
capture manifest must record the actual values used by that run.

### Update Metadata

`ledger_manager/src/firmware.rs` models three important metadata groups:

- `OsuFirmware` contains `perso`, `firmware`, `firmware_key`, `hash`, and the
  identifier of the final SE firmware.
- `FinalFirmware` contains the same installation selectors plus version, size,
  compatible MCU IDs, applications, providers, and device versions.
- `McuVersion` contains the MCU ID, version name, required bootloader version,
  compatible devices, and compatible final SE firmware IDs.

The fields named `firmware` and `firmware_key` are passed to Script Runner.
They must not be assumed to contain a plaintext image or a locally usable
decryption key.

### REST Endpoints

The existing firmware implementation calls these endpoints:

| Operation | Endpoint |
| --- | --- |
| Resolve device model | `/get_device_version` |
| Resolve installed firmware ID | `/get_firmware_version` |
| Find available OSU | `/get_latest_firmware` |
| Resolve an OSU by version | `/get_osu_version` |
| Resolve final firmware | `/firmware_final_versions/{id}` |
| List MCU versions | `/mcu_versions` |

The relevant functions begin around lines 331-518 of
`ledger_manager/src/firmware.rs`.

### Script Runner Operations

The update uses two Script Runner paths:

| Phase | Path | Parameters used by this repository |
| --- | --- | --- |
| Install OSU | `/install` | `targetId`, `firmware`, `firmwareKey`, `perso`, `livecommonversion` |
| Flash MCU | `/mcu` | `targetId`, `version`, `livecommonversion` |
| Install final firmware | `/install` | `targetId`, `firmware`, `firmwareKey`, `perso`, `livecommonversion` |

The call sites are `install_osu()`, `flash_mcu()`, and
`install_final_firmware()` around lines 746-831 of `firmware.rs`.

### HSM Message Flow

`query_via_websocket_with_progress()` connects with Tungstenite and handles:

- `exchange`: one HSM-provided APDU is sent to the device.
- `bulk`: a list of HSM-provided APDUs is sent sequentially.
- `success`: the operation completed.
- `error`: Script Runner reported failure.
- `warning`: Script Runner reported a non-fatal condition.

For `exchange`, the client returns the device response data and status to the
HSM. For `bulk`, this repository currently ignores individual response bodies
and reports success after processing the list. Capture code must preserve each
bulk response even though the current protocol handler does not use it.

The single exchange is around line 640 and the bulk exchange is around line
688 of `firmware.rs`.

### Update Sequence

The existing `update_firmware()` sequence is:

1. Query current device information.
2. Install the OSU through Script Runner.
3. Wait for a reboot when an MCU update is required.
4. Flash a compatible MCU, with at most five iterations.
5. Query the device again and require an OSU version string.
6. Install the final firmware through Script Runner.
7. Report completion.

The implementation is around lines 866-960 of `firmware.rs`.

### Existing Dependencies

The desktop implementation already uses:

- `ledger-apdu` for APDU structures.
- `ledger-transport-hidapi` for native USB HID exchange.
- `minreq` with HTTPS and Serde JSON for Manager API requests.
- `tungstenite` with Rustls native roots for Script Runner WebSockets.
- `serde` and `serde_json` for protocol records.
- `hex` for APDU serialization.
- `form_urlencoded` for Script Runner query construction.

A new standalone project can reuse these dependencies or substitute equivalent
HTTP, WebSocket, and HID implementations while preserving capture semantics.

## Proposed Project Architecture

Use separate acquisition and analysis components:

```text
ledger-firmware-audit/
|-- Cargo.toml
|-- src/
|   |-- main.rs
|   |-- config.rs
|   |-- manifest.rs
|   |-- device.rs
|   |-- manager_api.rs
|   |-- script_runner.rs
|   |-- capture/
|   |   |-- mod.rs
|   |   |-- record.rs
|   |   `-- writer.rs
|   `-- analyze/
|       |-- mod.rs
|       |-- apdu.rs
|       |-- classify.rs
|       `-- reconstruct.rs
|-- tests/
|   |-- capture_records.rs
|   `-- fixtures/
|-- captures/               # ignored by Git
`-- docs/
    `-- protocol-notes.md
```

The acquisition component should remain a transparent protocol relay. The
analysis component must operate offline from immutable capture files.

## Capture Configuration

Use explicit configuration rather than enabling collection by default:

```text
LEDGER_FIRMWARE_CAPTURE_DIR=./captures/<run-id>
LEDGER_FIRMWARE_CAPTURE_CONFIRM=I_UNDERSTAND_THIS_DEVICE_MAY_REQUIRE_RECOVERY
```

Recommended optional settings:

```text
LEDGER_CAPTURE_METADATA_ONLY=1
LEDGER_CAPTURE_LOG_RESPONSES=1
LEDGER_CAPTURE_FSYNC=phase
RUST_LOG=ledger_firmware_audit=debug
```

The application must refuse active firmware capture unless the output directory
and confirmation value are both present. Metadata-only collection should not
open an installation WebSocket or send update APDUs.

## Capture Directory Format

```text
captures/<run-id>/
|-- manifest.json
|-- api/
|   |-- device-version.json
|   |-- current-firmware.json
|   |-- latest-firmware.json
|   |-- osu-firmware.json
|   |-- final-firmware.json
|   `-- mcu-versions.json
|-- websocket/
|   |-- osu.jsonl
|   |-- mcu.jsonl
|   `-- final-firmware.jsonl
|-- apdu/
|   |-- osu.jsonl
|   |-- mcu.jsonl
|   `-- final-firmware.jsonl
|-- candidates/
|-- network/
|-- logs/
`-- SHA256SUMS
```

Create capture directories with user-only permissions. Add `/captures` to the
new project's `.gitignore`.

## Manifest Requirements

The manifest should contain:

```json
{
  "schema_version": 1,
  "capture_id": "2026-09-18-nanox-source-to-target",
  "started_at": "2026-09-18T12:00:00Z",
  "completed_at": null,
  "completed": false,
  "tool_git_commit": "<commit>",
  "device_model": "Nano X",
  "target_id": "0x00000000",
  "se_target_id": "0x00000000",
  "source_os_version": "<version>",
  "source_mcu_version": "<version>",
  "target_os_version": "<version>",
  "manager_api_v1": "https://manager.api.live.ledger.com/api",
  "script_runner": "wss://scriptrunner.api.live.ledger.com/update",
  "livecommonversion": "34.0.0",
  "provider": 1,
  "seedless_test_device": true,
  "capture_mode": "passive"
}
```

Update the manifest after every phase. Never mark a run complete until the
device returns to its dashboard and post-update device information succeeds.

## Capture Record Schema

Use append-only JSON Lines. Every record should include enough context to be
independently verified:

```json
{
  "schema_version": 1,
  "sequence": 42,
  "timestamp": "2026-09-18T12:01:23.456Z",
  "phase": "final_firmware",
  "layer": "apdu",
  "direction": "hsm_to_device",
  "hsm_nonce": 18,
  "bulk_index": 37,
  "bulk_total": 941,
  "payload_hex": "e0000000...",
  "payload_length": 235,
  "status": null,
  "sha256": "<hex digest>"
}
```

Use separate records for the command and device response. Preserve raw values
in addition to parsed APDU fields so parser errors can be corrected offline.

## Implementation Plan

### Phase 1: Passive Capture Foundation

1. Implement configuration parsing and explicit safety confirmation.
2. Implement atomic manifest creation and update.
3. Implement a `CaptureSink` interface with file and no-op implementations.
4. Implement monotonically increasing record sequence numbers.
5. Implement SHA-256 calculation over exact stored payload bytes.
6. Implement JSONL append, flush, and phase-boundary synchronization.
7. Ensure capture errors stop before an update starts rather than silently
   producing an incomplete audit.

Suggested interface:

```rust
trait CaptureSink {
    fn api_response(&mut self, endpoint: &str, status: u16, body: &[u8])
        -> Result<(), CaptureError>;
    fn websocket_message(&mut self, record: WebSocketCapture<'_>)
        -> Result<(), CaptureError>;
    fn apdu_command(&mut self, record: ApduCommandCapture<'_>)
        -> Result<(), CaptureError>;
    fn apdu_response(&mut self, record: ApduResponseCapture<'_>)
        -> Result<(), CaptureError>;
    fn phase_complete(&mut self, phase: CapturePhase)
        -> Result<(), CaptureError>;
}
```

### Phase 2: Manager API Capture

1. Read each HTTP response as raw bytes before deserializing it.
2. Record endpoint path, query parameter names, HTTP status, body length, and
   body SHA-256.
3. Write the exact body to `api/`.
4. Parse the same preserved body into typed metadata.
5. Do not persist cookies, authorization values, or unrelated headers.
6. Add a metadata-only CLI command and verify that it sends no update APDUs.

### Phase 3: WebSocket Capture

1. Add an explicit `CapturePhase` argument to each Script Runner session.
2. Record the endpoint path and query parameters used to initiate the session.
3. Record raw inbound messages before JSON parsing.
4. Record raw outbound messages before transmission.
5. Record message type, HSM query, nonce, and bulk command count.
6. Record close frames, warnings, and protocol errors.
7. Preserve unknown text and binary frames rather than dropping them.

Query URLs contain update selectors such as `firmwareKey` and `perso`. Treat
the complete capture directory as sensitive audit material even though these
values are not wallet private keys.

### Phase 4: APDU Capture

1. Record each HSM command before deserialization.
2. Parse and record `CLA`, `INS`, `P1`, `P2`, data length, and data bytes.
3. Record the exact serialized command sent to HID.
4. Record every device response, including bulk responses currently ignored by
   the Bacca implementation.
5. Store response data and status words separately.
6. Associate each exchange with phase, HSM nonce, bulk index, and bulk total.
7. Do not log unrelated device traffic outside the firmware workflow.

### Phase 5: Offline Reconstruction

1. Verify all JSONL sequence numbers and record hashes.
2. Inventory unique `(CLA, INS, P1, P2)` combinations by phase.
3. Plot command and payload sizes over sequence order.
4. Identify initialization, chunk transfer, signature, commit, and reboot
   command families.
5. Search candidate payloads for explicit offsets and sequence counters.
6. Generate multiple reconstruction hypotheses without overwriting originals.
7. Record exactly which commands contributed to each candidate binary.
8. Compare candidate sizes and hashes with API metadata.

Candidate names should describe the hypothesis:

```text
candidates/final-all-data-fields.bin
candidates/final-largest-ins-family.bin
candidates/final-sequential-offsets.bin
candidates/osu-sequential-offsets.bin
candidates/mcu-sequential-offsets.bin
```

## Analysis Procedure

Start with non-destructive identification:

```bash
file "captures/<run-id>/candidates/final-sequential-offsets.bin"
shasum -a 256 "captures/<run-id>/candidates/final-sequential-offsets.bin"
strings -a "captures/<run-id>/candidates/final-sequential-offsets.bin"
xxd -l 256 "captures/<run-id>/candidates/final-sequential-offsets.bin"
binwalk "captures/<run-id>/candidates/final-sequential-offsets.bin"
```

Calculate whole-file and windowed entropy. Also inspect repeated blocks,
compression signatures, executable headers, vector-table-like values, and runs
of `00` or `ff` bytes.

Classify results conservatively:

| Observation | Initial classification |
| --- | --- |
| Recognizable executable structure | Possible plaintext firmware region |
| Standard compression header | Compressed container or image |
| Structured records plus signatures | Signed update container |
| Uniform high entropy, stable across runs | Probably static encrypted material |
| Uniform high entropy, changes per device | Probably device-specific wrapping |
| Uniform high entropy, changes per session | Probably session-specific wrapping |
| No coherent offsets or structure | Reconstruction hypothesis is likely wrong |

Concatenation alone is not evidence of successful decryption.

## Controlled Capture Matrix

Use repeated captures to determine whether update material is static,
device-bound, or session-bound:

| Test | Source | Target | Device | Purpose |
| --- | --- | --- | --- | --- |
| A1/A2 | Same | Same | Same | Detect session variation |
| B1/B2 | Same | Same | Different | Detect device personalization |
| C1/C2 | Different | Same | Different | Detect source-version dependence |
| D | Same | Same | Same | Metadata-only baseline |

Compare complete transcripts, individual APDUs, APDU data fields, and response
status sequences. Preserve failed runs because they can reveal phase boundaries
and validation behavior.

## Independent Observation

Use a packet capture to confirm hosts, connection timing, reconnects, and record
sizes:

```bash
sudo tcpdump -i any -s 0 -w "captures/<run-id>/network/update.pcap"
```

On macOS, select the actual active interface if `any` is unavailable. The
packet capture will normally contain encrypted TLS and is validation evidence,
not the primary plaintext source.

A TLS interception proxy may be added only after direct instrumentation works.
Use it to confirm that REST and WebSocket captures are complete. Do not weaken
TLS verification in production code, and do not expect TLS interception to
expose data decrypted only inside the Ledger device.

## Hardware Runbook

1. Factory-reset the spare device and confirm it has no valuable seed.
2. Record model, target ID, OS version, MCU version, and bootloader state.
3. Close Ledger Live and disconnect every other Ledger device.
4. Disable host sleep and connect stable power.
5. Start the packet capture and terminal log.
6. Run metadata-only collection and inspect the resulting manifest.
7. Confirm the target update and expected OSU/MCU phases.
8. Start passive update capture.
9. Record physical confirmations and reboot events in the run log.
10. Do not disconnect the device until it returns to the dashboard.
11. Query device information and perform a genuine check after completion.
12. Stop external captures and calculate `SHA256SUMS`.
13. Make the original run directory read-only before offline analysis.

## Validation and Acceptance Criteria

Before connecting hardware:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets
```

The capture implementation is ready for a hardware run when:

- Capture-disabled behavior matches the original updater.
- Metadata-only mode cannot open an installation WebSocket.
- Synthetic `exchange` and `bulk` fixtures produce complete transcripts.
- Long HSM payloads are preserved without truncation.
- Every APDU command has a corresponding response or explicit transport error.
- Sequence numbers and hashes survive an interrupted run.
- Unknown WebSocket messages are preserved.
- Captures never include unrelated signing or application traffic.

## Bacca Integration Notes

If implementing capture in the source repository before extracting it:

- Add capture hooks to `ledger_manager/src/firmware.rs`.
- Add configuration and user-facing commands in the CLI crate.
- Use `cli/src/operations.rs::perform_firmware_update()` as the active firmware
  update integration point.
- The interactive menu calls this operation from `cli/src/menu.rs`.
- Legacy `LEDGER_COMMAND=updatefirm` currently exits with "not yet implemented"
  in `cli/src/main.rs`; do not use that path without fixing its dispatch.
- Device reconnection helpers already exist for repair operations, but the main
  `update_firmware()` path retains one transport across reboot phases. Test this
  carefully for each device model before relying on unattended capture.
- The web crate can check firmware metadata but native firmware flashing is
  desktop-gated and uses `ledger-transport-hidapi`.

## Expected Result and Limitation

A successful project should produce a reproducible local record of:

- Firmware discovery metadata.
- OSU, MCU, and final-firmware Script Runner sessions.
- Every host-visible firmware update APDU.
- Every host-visible device response.
- One or more documented candidate update blobs.
- Evidence supporting a plaintext, compressed, signed, encrypted, device-bound,
  or session-bound classification.

The likely outcome is an encrypted or signed installation stream rather than a
plaintext Ledger OS executable. If plaintext is never present at the Manager
API, Script Runner, or USB HID boundaries, a network or host MITM cannot recover
it. That negative result is still useful: it identifies the device or OSU as
the encryption boundary and supports an audit of server dependence, update
ordering, personalization, rollback controls, and protocol reproducibility.

## Public Reference Material

- Ledger public OS components:
  <https://github.com/LedgerHQ/ledger-secure-os>
- Ledger Secure SDK:
  <https://github.com/LedgerHQ/ledger-secure-sdk>
- Ledger Bitcoin application:
  <https://github.com/LedgerHQ/app-bitcoin-new>
- Ledger OS hardware architecture:
  <https://developers.ledger.com/docs/device-app/explanation/ledger-os/hardware-architecture>

These references can help identify public interfaces and application code, but
they do not provide the complete proprietary production Secure Element OS.
