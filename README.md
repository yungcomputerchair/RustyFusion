# RustyFusion
RustyFusion is an open-source server emulator for Cartoon Network's MMO Fusionfall written in Rust inspired by the [OpenFusion project](https://github.com/OpenFusionProject) in which I am an active contributor. RustyFusion was initially an experiment for me to gain experience writing Rust but is now on course for eventual feature-completion. **Please note that, until then, RustyFusion is NOT ready for use as a production Fusionfall server!**

## RustyFusion vs. OpenFusion
- **Compatibility:** RustyFusion is designed to work with general-purpose Fusionfall clients, as it speaks the original Fusionfall network protocol. This means that [OpenFusionClient](https://github.com/OpenFusionProject/OpenFusionClient) can connect to a RustyFusion server with no extra work. RustyFusion also uses the same tabledata repository as OpenFusion for data sourcing, and it will later be compatible with the OpenFusion monitor protocol and database schema once database support is added.
- **Safety:** Because RustyFusion is written in Rust as opposed to OpenFusion's choice of C++, it is, in theory, **much less** prone to memory safety issues, security vulnerabilities, and undefined behavior than OpenFusion's implementation of the game with a near-zero decrease in performance.
- **Scalability:** Unlike OpenFusion, RustyFusion's login server and shard server are **two separate binaries** that communicate to each other over the network, allowing for a more flexible server architecture with multiple shard servers.
- **Reliability:** RustyFusion comes after years of writing, refactoring, and evaluating OpenFusion code. There were a handful of cut corners and bad design decisions made in the development of OF that this project aims to avoid. Some already implemented examples include the increased usage of high-level types, a proper logging system, strict error-handling, and stricter packet validation ("anti-cheat"). These changes should lead to a cleaner codebase with less bugs.

## What's Done and Left To Do (Roughly)
- [x] Barebones login server functionality (connection and character creation)
- [x] Barebones "land walker" shard server functionality (connection, basic GM commands, seeing other players & NPCs, etc)
- [x] Config and tabledata frameworks
- [ ] Database (account system and player persistence)
- [x] Chunking
  - [x] Framework
  - [x] Instancing (infected zones + other private instances)
- [ ] Travel
  - [x] S.C.A.M.P.E.R. (fast-travel)
  - [ ] ***Monkey Skyway System (wyvern style)***
  - [ ] Sliders (bus style)
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
  - [ ] Core combat loop + mob AI
  - [ ] Abilities and (de)buffs
    - [ ] Passive skills (including nano)
    - [ ] Active skills (including nano)
    - [ ] Gumballs + other usables
  - [ ] Mob drops
- [ ] E.G.G.s (the ones on the ground that buff you)
- [ ] Missions
- [ ] ***Entity pathing***
- [ ] Infected Zone races
- [x] Guide changing
- [ ] Admin features
  - [ ] Built-in admin commands
  - [ ] Custom command system
  - [ ] OpenFusion monitor protocol
- [x] Time machine
- [ ] Event system
  - [ ] Fuse boss fight
  - [ ] Scripting API (stretch goal)
- [ ] "Academy" (build 1013) support (currently, only build 104 is supported)
  - [ ] Struct support
  - [ ] Patching framework
  - [ ] Dash skill
  - [ ] Nano capsules
  - [ ] Code redemption

Items that are ***highlighted*** are in planning or WIP. Some items have dependencies in other categories, so the list won't get completed in order.

## Developing
Cargo (Rust's package manager) makes working with Rust projects extremely easy to setup. Just clone the repo (recursively, to grab critical tabledata), build, and run:
```
git clone --recurse-submodules https://github.com/gsemaj/RustyFusion
cd RustyFusion
cargo build
cargo run --bin login_server # or shard_server
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
