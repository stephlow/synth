# Example patches

JSON documents loadable via rack **Load patch…** (or MCP `load_patch`).

| File | Description |
|------|-------------|
| `empty.json` | Same as engine **New patch** / `Patch::empty()` — Audio In/Out only |
| `groove.json` | Sequenced bass + kick + snare + MIDI lead (`Patch::example_groove()`) |

Regenerate after changing constructors:

```sh
cargo run -p dsp --example write_example_patches
```
