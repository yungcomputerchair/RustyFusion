# RustyFusion
RustyFusion is an open-source server emulator for Cartoon Network's MMO Fusionfall written in Rust inspired by the [OpenFusion project](https://github.com/OpenFusionProject) in which I am an active contributor. RustyFusion was initially an experiment for me to gain experience writing Rust but is now on course for eventual feature-completion. **Please note that, until then, RustyFusion is NOT ready for use as a production Fusionfall server!**

## RustyFusion vs. OpenFusion
- **Compatibility:** RustyFusion is designed to work with general-purpose Fusionfall clients, as it speaks the original Fusionfall network protocol. This means that [OpenFusionClient](https://github.com/OpenFusionProject/OpenFusionClient) can connect to a RustyFusion server with no extra work. RustyFusion's SQL backends also use a superset of the OpenFusion database schema, and RustyFusion uses the same tabledata repository as OpenFusion for data sourcing. It will also later support the OpenFusion monitor protocol.
- **Safety:** Because RustyFusion is written in Rust as opposed to OpenFusion's choice of C++, it is, in theory, **much less** prone to memory safety issues, security vulnerabilities, and undefined behavior than OpenFusion's implementation of the game with a near-zero decrease in performance.
- **Scalability:** Unlike OpenFusion, RustyFusion's login server and shard server are **two separate binaries** that communicate to each other over the network, allowing for a more flexible server architecture with multiple shard servers. On top of that, RustyFusion shards properly support **channels**. There are built-in client commands to check and switch channels, like `/chinfo` and `/chwarp`.
- **Reliability:** RustyFusion comes after years of writing, refactoring, and evaluating OpenFusion code. There were a handful of cut corners and bad design decisions made in the development of OF that this project aims to avoid. Some already implemented examples include the increased usage of high-level types, a proper logging system, strict error-handling, and stricter packet validation ("anti-cheat"). These changes should lead to a cleaner codebase with less bugs.

## What's Done and Left To Do (Roughly)
- [x] Core login server functionality
  - [x] Client connection
  - [x] Shard connection
  - [x] Auto account creation
  - [x] Character creation
  - [x] Character deletion
  - [x] Character selection
  - [x] Shard selection +
  - [x] Shard querying (channel + player info) +
- [x] Core shard server functionality
  - [x] Login server connection
  - [x] Client connection
  - [x] Channels +
  - [x] MOTD +
- [x] Config and tabledata frameworks
- [x] Core database functionality
  - [x] Framework +
  - [x] Account loading
  - [x] Player loading & saving
  - [x] Periodic shard auto-saving +
- [x] Chunking
  - [x] Framework
  - [x] Entity tracking
  - [x] Instancing (infected zones + other private instances)
- [x] Travel
  - [x] S.C.A.M.P.E.R. (fast-travel)
  - [x] Monkey Skyway System (wyvern style)
  - [x] Sliders (bus style)
  - [x] Vehicles
  - [x] Warping through NPCs
- [x] Items
  - [x] Framework (equipping, stacking, deleting, etc)
  - [x] Vendors (buying, selling, buy-backs)
  - [x] Croc-Potting
  - [x] Trading
  - [x] C.R.A.T.E. opening
- [ ] Social features
  - [x] Basic chat
  - [ ] Friends
  - [ ] Groups
  - [ ] Email system
- [x] Nano framework
  - [x] Swapping equipped nanos
  - [x] Summoning nanos
  - [x] Acquiring nanos
  - [x] Changing nano powers
- [ ] Combat
  - [x] Mobs
  - [ ] Core combat loop & mob AI
  - [ ] Abilities and (de)buffs
    - [ ] Passive skills (including nano)
    - [ ] Active skills (including nano)
    - [ ] Gumballs & other usables
    - [ ] E.G.G.s (the ones on the ground that buff you)
  - [x] Mob drops
- [x] Missions
  - [x] Starting tasks +
  - [x] Switching active mission
  - [x] Quest items
  - [x] Completing tasks +
  - [x] Mission rewards
  - [x] Escort tasks +
      - [x] Eduardo (Scary Monsters)
      - [x] Billy (Carnival Collection)
      - [ ] Professor Utonium (New Creep in Town (Part 3 of 3))
      - [ ] Grim (Don't Fear the Reaper (Part 4 of 4))
- [x] Entity pathing
- [ ] Infected Zone races
- [x] Guide changing
- [ ] Admin features
  - [x] Built-in admin commands +
  - [ ] Custom command system
    - [ ] Account banning
  - [ ] OpenFusion monitor protocol
  - [ ] Interactive terminal (bonus)
- [x] Time machine
- [ ] Event system
  - [ ] Fuse boss fight
  - [ ] Scripting API (bonus)
- [ ] "Academy" (build 1013) support (currently, only build 104 is supported)
  - [ ] Struct support
  - [ ] Patching framework
  - [ ] Dash skill
  - [ ] Nano capsules
  - [ ] Code redemption

### Known Issues
None currently

Items that are ***highlighted*** are in planning or WIP. Items marked with `+` are either new and not present in OpenFusion or enhanced from OpenFusion (bug fixes not included). Some items have dependencies in other categories, so the list won't get completed in order.

## Developing
**RustyFusion requires an instance of a supported database backend to connect to for database operations.** You can manually configure an instance and set the connection parameters in `config.toml` or, for PostgreSQL specifically, use `docker compose` to spawn a container with the default parameters.

Cargo (Rust's package manager) makes working with Rust projects extremely easy to setup. Just clone the repo (recursively, to grab critical tabledata), build, and run:
```
git clone --recurse-submodules https://github.com/gsemaj/RustyFusion
cd RustyFusion
cargo build
cargo run --bin login_server # or shard_server
```

### Database Backend
RustyFusion compiles with the PostgreSQL backend by default. If you'd like to compile RustyFusion to use a specific database backend (such as MongoDB), run the following instead of `cargo build`:
```
cargo build --no-default-features --features <mongo|postgres>
```

## Contributing
If you have code you want to contribute, make sure you follow the general code style and run the following commands before you commit your code (CI/CD will catch you if you don't):
```
cargo fmt
cargo clippy
```

## Other Notes
RustyFusion is a **personal project that I work on in my free time**, so it is unlikely to progress at a constant pace.

Feel free to email me or ping me @ycc on the [OpenFusion Discord](https://discord.gg/DYavckB) if you have any questions about the project or are interested in contributing.
