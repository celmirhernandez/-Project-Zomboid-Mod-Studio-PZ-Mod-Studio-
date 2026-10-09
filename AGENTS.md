# AGENTS.md — PZ Mod Studio

Desktop suite (Tauri 2 + React 19 + Rust) for Project Zomboid mod management: VFS conflict detection, 3-way Lua AST merge, load-order sorting, crash diagnostics, and a dedicated MCP server for AI agents.

Windows-only in practice (registry probes, `tasklist`/`taskkill`, `ProjectZomboid64.exe`).

## Commands

```bash
npm install                              # Node 22+
npm run tauri dev                        # full app; Vite on :1420 (strictPort), Rust backend hot-reloads
npm run build                            # tsc && vite build  -> dist/  (this IS the typecheck gate)
npm run build:portable                   # tauri build --no-bundle, then copies both .exe to repo root

# Rust only — note the manifest path; src-tauri is its own crate, not a workspace member of the root
cargo build --release --bin pz-mcp-server --manifest-path "src-tauri/Cargo.toml"
cargo build --release --manifest-path "src-tauri/Cargo.toml"     # pz-mod-studio (default-run)
cargo test  --manifest-path "src-tauri/Cargo.toml"               # only 4 unit tests exist (load_order/mod_info.rs, patch_generator/mod.rs)
```

**No lint, no formatter, no JS test runner, no CI typecheck.** `npm run build` failing on `tsc` is the only frontend gate; don't look for a `test`/`lint` script that doesn't exist. `tsconfig.json` is strict + `noUnusedLocals` + `noUnusedParameters`, so dead imports break the build.

## Repo layout & ownership

| Path | Owns |
| :--- | :--- |
| `src/App.tsx` | Top-level navigation: owns `activeTab` state and conditionally renders modules. |
| `src/components/<feature>/` | One directory per feature. Top-level tabs: `instances`, `load_order`, `server`, `merger`, `sandbox`, `settings`. `polyfills` and `presets` are sub-features rendered inside other modules, not tabs. |
| `src/services/tauri.ts` | **Single typed IPC bridge.** Every `#[tauri::command]` needs a wrapper here or the frontend can't reach it. |
| `src/data/default_rules.ts` | B41→B42 polyfill rule catalog (see contracts below). |
| `src-tauri/src/lib.rs` | `#[tauri::command]` definitions + `invoke_handler` registration. |
| `src-tauri/src/mcp/` | MCP protocol (`protocol.rs`, `server.rs`) and all tool schemas/handlers (`tools.rs`). |
| `src-tauri/src/patch_generator/` | Master patch packaging. **2919 lines, the core engine.** |
| `src-tauri/src/load_order/mod_info.rs` | mod.info parsing + path resolution. 1214 lines. |
| `mods/Z_PZModStudio_Bridge/` | Reference copy of the companion mod. **Not the install source** — see below. |
| `scripts/package-portable.js` | Copies `src-tauri/target/release/*.exe` to repo root, rewrites `README_PORTABLE.txt`. |

Adding a top-level tab touches three places: the `ActiveTab` union in `src/types/index.ts`, the conditional render in `src/App.tsx`, and `src/components/layout/StudioSidebar.tsx`.

## Contracts that break silently

- **The bridge companion mod is embedded as inline Rust string literals.** `install_bridge_companion_mod()` in `src-tauri/src/sandbox/mod.rs` writes `PZModStudio_Bridge.lua` from a `r#"..."#` literal in that file (plus a server copy, `Z_PZModStudio_Polyfills.lua`, and B41/B42 `mod.info` variants). Editing `mods/Z_PZModStudio_Bridge/**` changes nothing at runtime.
- **The runtime polyfill shim is also a Rust string**, `generate_master_polyfill_lua()` in `src-tauri/src/patch_generator/mod.rs` (~2900-line Lua literal). Adding a rule to `src/data/default_rules.ts` only enables it in the UI.
- **Polyfill IDs are a cross-file string contract.** `polyfill_rule_id_suggestion` values in `src-tauri/src/sandbox/mod.rs` (`B42_OBJECT_INTERACT_SAFETY`, `B42_BANDITS_ROOM_SAFETY`, `B42_DAILY_REPORT_SAFETY`, `SAFE_GLOBAL_TABLE_ACCESS`, `SANITIZE_TRANSLATOR_FORMAT`) must exactly match `id` fields in `src/data/default_rules.ts`. Rename one side and the "Apply Fix" button silently does nothing.
- **Default package name mismatch.** `patch_generator` defaults to `Z_PZModStudio_MergedPatch`, but MCP `save_draft_resolution` (`mcp/tools.rs:618`) defaults `package_folder_name` to `"MasterPatch"`. Always pass `package_folder_name: "Z_PZModStudio_MergedPatch"` explicitly to MCP patch tools.
- **Tauri arg naming is snake_case → camelCase.** Rust `user_zomboid_dir` must be invoked as `{ userZomboidDir }` in `tauri.ts`. Silent `undefined` otherwise.
- **IPC bridge is file-based, not a socket.** Commands go out via `pz_ipc_queue.json` / `pz_server_commands.json`, responses come back via `pz_ipc_resp.json`; the mod processes the queue every 10 ticks and player telemetry every 30. Requires `install_bridge_companion_mod` and a running game.

## MCP server

Two entrypoints, same code path:
- `pz-mcp-server.exe` — standalone console binary (`src-tauri/src/bin/mcp_server.rs`).
- `Project-Zomboid-Mod-Studio.exe --mcp` (also `--mcp-server` / `mcp`) — `src-tauri/src/main.rs`.

Speaks MCP 2024-11-05, JSON-RPC 2.0 over stdio. Client config:

```json
{ "mcpServers": { "pz-mod-studio": {
  "command": "C:\\Path\\To\\Project-Zomboid-Mod-Studio-PZ-Mod-Studio-\\pz-mcp-server.exe", "args": [] } } }
```

23 tools (`src-tauri/src/mcp/tools.rs` — trust this file over any doc listing):
- Paths/mods: `get_studio_paths`, `list_installed_mods`, `sort_mod_load_order`, `scan_mod_conflicts`, `scan_mod_diagnostics`
- Profiles: `list_mod_profiles`, `create_mod_profile`, `activate_mod_profile`
- Game/IPC: `get_game_status`, `launch_game`, `terminate_game`, `send_game_ipc_command`, `get_game_ipc_response`, `install_bridge_companion_mod`
- Logs/diagnostics: `get_monitor_logs`, `list_available_logs`, `read_log_file`, `get_crash_diagnostics`
- Merge/patch: `validate_lua_syntax`, `merge_lua_scripts`, `get_master_patch_status`, `save_draft_resolution`, `list_merged_packages`

8 resources: `pz://monitor/console-log`, `pz://mods/installed-summary`, `pz://paths/config`, `pz://patches/status`, `pz://game/status`, `pz://profiles/list`, `pz://conflicts/active`, `pz://diagnostics/latest-crash`.

## Build & release gotchas

- `Project-Zomboid-Mod-Studio.exe` and `pz-mcp-server.exe` are **committed to the repo root** on purpose (direct-download distribution) and `.gitignore` does not exclude them. Expect them in `git status` after any local release build.
- `npm run build:portable` warns instead of failing if the root `.exe` is locked (EBUSY) — close the running app first.
- `.github/workflows/release.yml` fires only on `v*` tags and uses `tauri-apps/tauri-action`; it does **not** run `scripts/package-portable.js`, so CI-published assets differ from a local `build:portable` run.
- `tauri.conf.json` hard-codes `beforeBuildCommand: npm run build` and `devUrl: http://localhost:1420`.

## Docs — read before touching PZ-side logic

- `docs/INDEX.md` + 9 chapters: B41/B42 engine internals, Kahlua VM, Lua lifecycle, crafting, UI, loot, ModData, sound, crash/VFS, IPC bridge. Check the B41-vs-B42 chapter before writing any Lua or polyfill.
- `DOCUMENTACION_SISTEMA.md` (242 lines, ES): per-module description of every Rust backend module — the most complete internal map.
- `PZ MODDING AGENT GUIDE.md` (692 lines, ES): long-form PZ modding manual. Overlaps `docs/`; prefer `docs/` for current info.
- `CONTRIBUTING.md`: setup + contribution workflow.