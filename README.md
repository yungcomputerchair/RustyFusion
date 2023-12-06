# RustyFusion
RustyFusion is an open-source server emulator for Cartoon Network's MMO FusionFall written in Rust. Inspired by the [OpenFusion project](https://github.com/OpenFusionProject) in which I am an active contributor. RustyFusion was initially just an experiment for me to gain experience writing Rust but is now on a trajectory toward eventual feature-completion. **Please note that, until then, RustyFusion is NOT ready for use as a production FusionFall server!**

## RustyFusion vs. OpenFusion
- **Compatibility:** RustyFusion is designed to work with general-purpose FusionFall clients, as it speaks the original FusionFall network protocol. This means that [OpenFusionClient](https://github.com/OpenFusionProject/OpenFusionClient) can connect to a RustyFusion server with no extra work. RustyFusion also uses the same tabledata repository as OpenFusion for data sourcing, and it will later be compatible with the OpenFusion monitor protocol and database schema once database support is added.
- **Safety:** Because RustyFusion is written in Rust as opposed to OpenFusion's choice of C++, it is, in theory, **much less** prone to memory safety issues, security vulnerabilities, and undefined behavior than OpenFusion's implementation of the game with near-zero decrease in performance.
- **Scalability:** Unlike OpenFusion, RustyFusion's login server and shard server are **two separate binaries** that communicate to each other over the network, allowing for a more flexible server architecture with multiple shard servers.
- **Reliability:** RustyFusion comes after years of writing, refactoring, and evaluating OpenFusion code. There were a handful of cut corners and bad design decisions made in the development of OF that this project aims to avoid. Some already implemented examples include the increased usage of high-level types, a proper logging system, and strict error-handling. These changes should lead to a cleaner codebase with less bugs.

## What's Done and Left To Do (Roughly)
- [x] Barebones login server functionality (connection and character creation)
- [x] Barebones "land walker" shard server functionality (connection, basic GM commands, seeing other players & NPCs, etc)
- [x] Config and tabledata frameworks
- [ ] Database (account system and player persistance)
- [ ] Chunking
  - [x] Framework
  - [ ] Instancing (infected zones + other private instances)
- [ ] Travel
  - [ ] S.C.A.M.P.E.R. (fast-travel)
  - [ ] Monkey Skyway System (wyvern style)
  - [ ] Sliders (bus style)
- [ ] Items
  - [x] Framework (equipping, stacking, deleting, etc)
  - [x] Vendors (buying, selling, buy-backs)
  - [ ] Croc-Potting
  - [ ] Trading
  - [ ] C.R.A.T.E.s
- [ ] Social features
  - [x] Basic chat
  - [ ] Friends
  - [ ] Groups
  - [ ] Email system
- [ ] Nano framework
- [ ] Combat
  - [ ] Mobs
  - [ ] Core combat loop + mob AI
  - [ ] Abilities and (de)buffs
- [ ] E.G.G.s
- [ ] Missions
- [ ] Entity pathing
- [ ] Admin features
  - [ ] Custom command system
  - [ ] OpenFusion monitor protocol
- [ ] "Academy" (build 1013) support (currently, only build 104 is supported)

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
