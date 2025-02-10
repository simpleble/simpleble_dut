SimpleBLE - Device Under Test
=============================

This projets holds the source code for the Device Under Test (DUT) for the SimpleBLE project,
used for Hardware-in-the-Loop (HITL) testing.

Instructions
------------

To use this project, you'll need:

- MakerDiary nRF52840 M2 Dev Kit: https://makerdiary.com/products/nrf52840-m2-developer-kit
- Probe.rs: https://probe.rs/
- Rust toolchain: https://rustup.rs/

To flash the device, initially you'll need to flash softdevice running:

```bash
./scripts/flash-softdevice.sh
```

To flash the application, run:

```bash
cargo run
```
