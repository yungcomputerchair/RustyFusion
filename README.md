# RustyFusion
RustyFusion is an open-source server emulator for Cartoon Network's MMO Fusionfall written in Rust inspired by the [OpenFusion project](https://github.com/OpenFusionProject) in which I am an active contributor. RustyFusion was initially an experiment for me to gain experience writing Rust but is now on course for eventual feature-completion. **Please note that, until then, RustyFusion is NOT ready for use as a production Fusionfall server!**

## RustyFusion vs. OpenFusion
- **Compatibility:** RustyFusion is designed to work with general-purpose Fusionfall clients, as it speaks the original Fusionfall network protocol. This means that [OpenFusionClient](https://github.com/OpenFusionProject/OpenFusionClient) can connect to a RustyFusion server with no extra work. RustyFusion's PostgreSQL backend also uses the OpenFusion database schema, and RustyFusion uses the same tabledata repository as OpenFusion for data sourcing. It will also later support the OpenFusion monitor protocol.
- **Safety:** Because RustyFusion is written in Rust as opposed to OpenFusion's choice of C++, it is, in theory, **much less** prone to memory safety issues, security vulnerabilities, and undefined behavior than OpenFusion's implementation of the game with a near-zero decrease in performance.
- **Scalability:** Unlike OpenFusion, RustyFusion's login server and shard server are **two separate binaries** that communicate to each other over the network, allowing for a more flexible server architecture with multiple shard servers.
- **Reliability:** RustyFusion comes after years of writing, refactoring, and evaluating OpenFusion code. There were a handful of cut corners and bad design decisions made in the development of OF that this project aims to avoid. Some already implemented examples include the increased usage of high-level types, a proper logging system, strict error-handling, and stricter packet validation ("anti-cheat"). These changes should lead to a cleaner codebase with less bugs.
- **Completeness:** OpenFusion is not technically complete in and of itself. There's a handful of features that remain unimplemented at the time of writing, such as channels, certain built-in commands, and NPC v. NPC combat. RustyFusion aims to close the gap on as many of these features as possible.

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
  - [ ] Buddies
    - [x] Framework
    - [x] Buddy chat (https://github.com/yungcomputerchair/RustyFusion/pull/11, thanks **lwcasgc**!)
    - [x] Buddy warping (https://github.com/yungcomputerchair/RustyFusion/pull/12, thanks **lwcasgc**!)
    - [ ] Emails
    - [ ] Blocking
  - [x] Groups
    - [x] Framework +
    - [x] Group chat
    - [x] Shared kills
    - [x] Group warping
- [x] Nanos*
  - [x] Swapping equipped nanos
  - [x] Summoning nanos
  - [x] Acquiring nanos
  - [x] Changing nano powers
- [ ] Combat
  - [x] Mobs
  - [x] Core combat loop & mob AI +
  - [x] Player respawning
  - [ ] Abilities and (de)buffs
    - [ ] Passive skills (including nano)*
    - [ ] Active skills (including nano)*
    - [ ] Gumballs & other usables
    - [ ] E.G.G.s (the ones on the ground that buff you)
  - [ ] Rockets and grenades
  - [x] Mob drops
- [x] Missions
  - [x] Starting tasks +
  - [x] Switching active mission
  - [x] Quest items
  - [x] Completing tasks +
  - [x] Mission rewards
  - [x] Escort tasks +
      - [x] Follow player
      - [x] Follow path
        - [x] Eduardo (Scary Monsters)
        - [x] Billy (Carnival Collection)
        - [x] Grim (Don't Fear the Reaper (Part 4 of 4))
- [x] Entity pathing
- [ ] Infected Zones
  - [ ] Movement elements
  - [ ] Races
    - [ ] Rank endpoint
- [x] Guide changing
- [ ] Admin features
  - [x] Built-in cheat commands +
  - [x] Custom command system
  - [x] Account (un)banning
  - [ ] **OpenFusion monitor protocol using [ffmonitor](https://github.com/OpenFusionProject/ffmonitor)**
    - [x] `player` events
    - [x] `chat` events
    - [ ] `bcast` events
    - [ ] `email` events
    - [x] `namereq` events
  - [ ] ofapi support
    - [] OpenFusion DB version 6 compliance
    - [] Login cookie support
- [x] Time machine
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
cargo run --bin login # for login server
cargo run --bin shard # for shard server
```
To force the server to load the config file from a location other than `config.toml`, you can override it in the command line:
```
cargo run --bin shard config1.toml
cargo run --bin shard config2.toml
```

## Contributing
If you have code you want to contribute, make sure you follow the general code style and run the following commands before you commit your code (CI/CD will catch you if you don't):
```
cargo fmt
cargo clippy
```

## Other Notes
### On Creative Liberties
Although RustyFusion tries to match the front-end behavior of OpenFusion closely, there are some notable, intentional differences that exist between the two. I only diverged on these because I think they lead to an overall improved experience.
- Mob aggro is dynamic and builds up/diminishes according to different factors. This new formula is being tweaked often.
- Not all OpenFusion custom commands will be supported (particularly "gruntwork" commands) and brand new custom commands are added as good use cases come up.

### Disclaimer
RustyFusion is a **personal project that I work on in my free time**, so it is unlikely to progress at a constant pace.

Feel free to email or DM me if you have any questions about the project or are interested in contributing.
