# RustyFusion – Copilot Instructions

## Project Overview

RustyFusion is an open-source server emulator for Cartoon Network's MMO **FusionFall**, written in Rust. It is inspired by [OpenFusion](https://github.com/OpenFusionProject) and aims for eventual feature-completion with a cleaner, safer codebase. It speaks the original FusionFall network protocol and uses the OpenFusion PostgreSQL database schema.

The server is split into **two separate binaries**:
- **Login server** (`src/bin/login/`) – handles authentication, character selection, and shard routing.
- **Shard server** (`src/bin/shard/`) – handles the live game world (entities, combat, travel, items, etc.).

---

## Build, Lint, and Format

```bash
cargo build          # build (required before running)
cargo clippy         # lint (must pass before committing)
cargo fmt            # format (must pass before committing)
cargo run --bin login   # run login server
cargo run --bin shard   # run shard server
```

Unit tests exist in several modules (`util`, `config`, `chunk`, `net/mod`, `net/crypto`, `entity/mod`, `tabledata`). Run them with `cargo test`. Do not remove existing tests.

The minimum supported Rust version (MSRV) is **1.88.0**, edition **2021**, as declared in `Cargo.toml`.

CI (`rust.yml`) enforces both `cargo build` and `rustfmt` on push/PR to `main`. Always run `cargo fmt` and `cargo clippy` before committing.

---

## Repository Layout

```
src/
  lib.rs              # library root; declares all public modules
  defines.rs          # numeric constants (PROTOCOL_VERSION, ranges, IDs, etc.)
  enums.rs            # game enums (ItemType, CombatStyle, etc.) via ffenum! macro
  error.rs            # FFError, FFResult, logging utilities
  config.rs           # config framework (generated from config_schema.toml)
  tabledata.rs        # game data loaded from JSON tabledata submodule
  helpers.rs          # shared shard helper functions (broadcast, group, etc.)
  util.rs             # general utilities (clamp, rand, timers, Bitfield, etc.)
  chunk.rs            # world chunking, InstanceID, EntityMap
  ai.rs               # NPC AI script name selection
  geo.rs              # geo-IP-based shard routing
  monitor.rs          # OpenFusion monitor protocol
  tui.rs              # ratatui-based terminal UI
  timer.rs            # interval timer utilities
  path.rs             # NPC/entity pathing
  item.rs             # item types and logic
  nano.rs             # nano types and logic
  mission.rs          # mission/task types
  skills.rs           # skill/buff system
  trade.rs            # trade context
  net/                # networking layer
    mod.rs            # PacketCallback, DisconnectCallback, ClientMap types
    ffclient.rs       # FFClient handle (cheap to clone, send-safe)
    ffconnection.rs   # raw TCP connection handling
    ffserver.rs       # FFServer (async TCP listener + event loop)
    packet.rs         # Packet, FFPacket trait, PacketID enum (auto-generated)
    crypto.rs         # encryption logic
  entity/             # game entity types
    mod.rs            # Entity trait, Combatant trait, EntityID enum
    player.rs         # Player struct
    npc.rs            # NPC struct
    slider.rs         # Slider entity
    egg.rs            # E.G.G. entity
  state/              # server state
    mod.rs            # shared state utilities
    shard.rs          # ShardServerState
    login.rs          # LoginServerState
  database/           # database layer
    mod.rs            # DbImpl trait, db_get(), db_init(), macro-generated API
    postgresql.rs     # PostgreSQL implementation
  scripting/          # Luau scripting for NPC AI
    mod.rs            # scripting_init(), Lua environment setup, luau_* macros
    entity.rs         # entity Lua bindings
    npc.rs            # NPC-specific Lua bindings
  bin/
    login/            # login server binary
      main.rs
      login.rs        # login packet handlers
      shard.rs        # shard↔login connection handlers
    shard/            # shard server binary
      main.rs
      pc.rs           # player connection handlers
      nano.rs         # nano handlers
      item.rs         # item/vendor/trade handlers
      combat.rs       # combat handlers
      mission.rs      # mission handlers
      npc.rs          # NPC interaction handlers
      chat.rs         # chat handlers
      buddy.rs        # buddy system handlers
      group.rs        # group handlers
      gm.rs           # GM/admin command handlers
      transport.rs    # travel/transport handlers
      trade.rs        # trade handlers
config_schema.toml    # source of truth for config struct (auto-generates config code)
build.rs              # code generation (config + Luau type stubs)
tabledata/            # git submodule with JSON game data (xdt.json, NPCs, paths, etc.)
scripts/              # Luau scripts for NPC AI behaviors
sql/                  # SQL schema files
docker-compose.yml    # PostgreSQL dev container
```

---

## Key Types and Patterns

### Error Handling

All fallible functions return `FFResult<T>` = `Result<T, FFError>`.

```rust
// Build an error (non-disconnecting)
FFError::build(Severity::Warning, "message".to_string())

// Build a disconnecting error (closes the client connection)
FFError::build_dc(Severity::Warning, "message".to_string())

// Convert an enum parse error
FFError::from_enum_err(val)

// Chain errors
err.with_parent(inner_err)
```

Severity levels: `Debug`, `Info`, `Warning`, `Fatal`.

Logging utilities:
```rust
log(Severity::Info, "message");     // log a plain message
log_error(err);                     // log an FFError
log_if_failed(result);              // log and swallow errors
panic_if_failed(result);            // log then panic on error
```

### Packet Handlers

Shard packet handlers that need async DB access use the async signature:
```rust
pub async fn handler_name(
    pkt: Packet,
    clients: &ClientMap<'_>,
    state_lock: Arc<Mutex<ShardServerState>>,
    time: SystemTime,
) -> FFResult<()>
```

Sync handlers (most shard handlers) use:
```rust
pub fn handler_name(
    pkt: Packet,
    clients: &ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()>
```

Login server handlers use `LoginServerState` instead of `ShardServerState`.

**Reading a packet:**
```rust
let pkt: &sP_CL2FE_REQ_SOME_ACTION = pkt.get()?;
```

**Sending a packet:**
```rust
client.send_packet(P_FE2CL_REP_SOME_ACTION_SUCC, &resp_struct);
```

**Broadcasting to nearby entities:**
```rust
state.entity_map.for_each_around(EntityID::Player(pc_id), |client| {
    client.send_packet(P_FE2CL_SOME_BCAST, &bcast_struct);
});
```

### Packet Naming Convention

Packets follow the OpenFusion naming convention:
- `P_CL2LS_` – client → login server
- `P_LS2CL_` – login server → client
- `P_CL2FE_` – client → shard (FE = Frontend)
- `P_FE2CL_` – shard → client
- `P_FE2LS_` – shard → login server
- `P_LS2FE_` – login server → shard
- `REQ_` = request, `REP_` = reply, `SUCC` = success, `FAIL` = failure

Packet structs (e.g. `sP_CL2FE_REQ_NANO_EQUIP`) are auto-generated and live in `src/net/packet.rs`.

### Macros

```rust
unused!()        // expands to Default::default(); used for padding/unused packet fields
placeholder!(v)  // returns v; logs "PLACEHOLDER" in debug builds for unimplemented features
```

### Config

Config is generated at build time from `config_schema.toml` via `build.rs`. Access it with:
```rust
let config = config_get();
let val = config.general.some_setting.get();
```

Never edit the generated config code directly; edit `config_schema.toml`.

### Tabledata

Game data (items, NPCs, missions, nano stats, etc.) is loaded from the `tabledata/` submodule (JSON files). Access via:
```rust
let tdata = tdata_get();
let stats = tdata.get_npc_stats(npc_type);
```

The submodule must be checked out (`git clone --recurse-submodules` or `git submodule update --init`).

### Database

The database trait is defined via macro in `src/database/mod.rs`. Only PostgreSQL is currently supported. Initialization:
```rust
db_init(Severity::Fatal).await?;
let db = db_get();
db.some_operation(args).await?;
```

### Entity System

All game entities implement the `Entity` trait. Combat-capable entities additionally implement `Combatant`. Entity IDs are:
```rust
EntityID::Player(i32)
EntityID::NPC(i32)
EntityID::Slider(i32)
EntityID::Egg(i32)
```

Entities live in `ShardServerState.entity_map` (an `EntityMap`). Players are retrieved by pc_id:
```rust
state.get_player(pc_id)?      // &Player
state.get_player_mut(pc_id)?  // &mut Player
```

### FFClient

`FFClient` is a cheap-to-clone, thread-safe handle to a connected client. It holds a `Arc<RwLock<ClientMetadata>>` and a sender channel. Key methods:
```rust
client.send_packet(pkt_id, &pkt);
client.get_player_id()?
client.get_account_id()?
client.disconnect();
```

### Scripting (Luau)

NPC AI is driven by Luau scripts in `scripts/`. The `luau_type!`, `luau_function!`, `luau_class!`, and `luau_method!` macros in `src/scripting/mod.rs` both register Lua bindings and generate Luau type stubs (`scripts/globals.d.luau`) via `build.rs`.

### Chunking / Instancing

The world is divided into chunks. Each entity has a `ChunkCoords` with an `InstanceID` (channel, map, optional instance number). Entities are tracked in the `EntityMap`. Visibility is chunk-based.

---

## Code Style

- **Formatting**: enforced by `cargo fmt` (rustfmt defaults).
- **Blank lines**: add blank lines between multi-line blocks; do **not** add blank lines between adjacent single-line statements (e.g., `send_packet()` followed by `Ok(())`).
- **Comments**: only add comments matching the existing style or explaining non-obvious logic.
- **`unused!()`**: always use this macro for default/padding packet fields rather than `Default::default()` directly.
- **`placeholder!()`**: use for stubs of unimplemented but planned features.
- **Imports**: use `PacketID::*` glob when many packet IDs are needed. Use `crate::error::*` glob for error utilities in handler files.
- **Error propagation**: prefer `?` operator; use `log_if_failed()` when you want to swallow and log an error silently.

---

## Development Setup

1. **Clone recursively** to get the tabledata submodule:
   ```bash
   git clone --recurse-submodules https://github.com/yungcomputerchair/RustyFusion
   ```
2. **Database**: start a PostgreSQL instance. For local dev, use:
   ```bash
   docker compose up -d
   ```
   Or configure `config.toml` with your own PostgreSQL connection details.
3. **Config**: copy/create `config.toml` from the schema in `config_schema.toml`. The config file path can be overridden via command-line argument.
4. **Run**:
   ```bash
   cargo run --bin login
   cargo run --bin shard
   ```

---

## CI / GitHub Actions

The workflow at `.github/workflows/rust.yml` runs on push/PR to `main` (when relevant files change):
- **Build and test**: `cargo build --verbose` + `cargo test`
- **Formatting**: `rustfmt` check via `actions-rust-lang/rustfmt`
- **MSRV**: read from `Cargo.toml` via `actions-rust-lang/msrv`

Build artifacts (`target/debug/login`, `target/debug/shard`) are uploaded on push to `main`.

---

## Common Pitfalls

- The `tabledata/` directory is a **git submodule**. If it is empty, the server will fail to start. Run `git submodule update --init`.
- Config is code-generated; adding a new config option requires editing `config_schema.toml`, not the Rust source.
- Packet structs are code-generated from the protocol; do not manually add or edit them in `packet.rs`.
- The `scripting/mod.rs` macros (`luau_type!`, etc.) are parsed by `build.rs` to produce Luau type stubs. Their format must be preserved exactly.
- `FFClient::send_packet` takes `&self` — no mutable borrow needed to send packets.
- Packet struct fields use the original FusionFall naming convention (Hungarian notation: `i` = int, `s` = string, `e` = enum, etc.).
