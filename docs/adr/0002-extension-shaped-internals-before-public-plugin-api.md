# Use extension-shaped internals before a public plugin API

Lazydiff will design the **Diff Workspace** around extension-compatible concepts from the start — intents, effects, generic diff decorations, chrome/status slots, review workflow contributions, and feature contributions — but will not expose or depend on a full public plugin API initially. Fixed internal contributors keep phase one simple and Rust-friendly while preserving a migration path to ProseMirror/pi-style registered extensions once multiple independent features prove the need for that seam.

The product direction is "build your own diff / build your own code review": teams should eventually be able to customize review markers, commands, keybindings, inline rows, status chrome, review actions, and integrations without forking the safe core. The first step is bounded internal contribution seams, not arbitrary renderer mutation or third-party runtime plugins.
