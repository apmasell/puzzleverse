# Background for Developers
This provides some background for anyone wanting to modify the Puzzleverse
software. Extending the server will likely require setting up a server, so
reading [Background for Server Administrators](background-admin.md) is
recommended.

## Developer Setup
The game is build in Rust. Install the Rust toolchain using
[rustup](https://rustup.rs/). Then install the dependencies for Godot:

    apt install llvm-dev libclang-dev clang

You may want to install the Diesel database interface using:

    cargo install diesel_cli

You will need the WASM toolchain:

    rustup target add wasm32-unknown-unknown
    cargo install wasm-bindgen-cli

The build using:

    cargo build --target wasm32-unknown-unknown -p puzzleverse-client
    wasm-bindgen --target web target/wasm32-unknown-unknown/debug/puzzleverse-client.wasm  --out-dir server/ --no-typescript
    cargo build

## The Server/Client Split
Puzzleverse was designed to meet a few goals:

- security: malicious clients, malicious servers, and malicious realms should not disrupt functioning of the system
- consistency: players in the same realm should have a consistent experience; this means minimal glitching/teleporting. This necessarily requires keeping bandwidth demands low.
- resilience:  like any distributed system, parts of the system will be in a failed state and the surrounding infrastructure needs to continue operating
- limit cheating: make sure players with technical skills can't introspect the puzzle state

To this end, the sever/client split is somewhat different than most MMOs. The authoritatively server synchronises all behaviour in the game (rather than allowing clients to predict what will happen and try to reconcile on the server). To make the latency acceptable, the game dispenses with real-time navigation and physics.

Each realm has an associated state on the server. Some of that state is journaled to the server's database. Upon loading a realm, the server decomposes it into:

- the walk manifold; a collection of planes that the players can walk on with connection between them. Some of these paths can be controlled by puzzle elements.
- the puzzles elements (_puzzle pieces_)
- the puzzle rules
- the puzzle outputs
- location history and future of each player

The server and client have different information they need out of the realm description and they are going to have matched information on:

- the puzzle inputs
- the puzzle outputs

A server will create a realm and initialise all the puzzle pieces and journal
their state to the database. As players arrive, the server will place them at
the spawn point for the world. When a player wishes to move, the client will
compute a path the player wishes to walk and send it to the server. The server
will validate the path up to the point where it requires interaction with a
puzzle element. This is a players _committed path_, because the server is
convinced the player will walk it. It then sends this path, with timestamps, to
all clients. If the path involves interacting with a puzzle-controlled element,
the server will store the path for later. At the appropriate time, the server
will determine if the puzzle is in a state where the player is allowed to cross
a puzzle-controlled gate. If permitted, the server will commit the next chunk
of path and distribute it to the clients. Paths can include interaction with
puzzle pieces.

When a player interacts with a puzzle piece, the server will trigger the puzzle
piece to accept the interaction event. The piece can decide how to update its
state. There are also puzzle pieces that operate on timers. When a puzzle piece
is updated, it can emit events about its new state. The rules are used to
propagate events from one puzzle piece to the next. To prevent malicious realms
from overloading a server, there is a maximum number of rounds of updates that
a single event can produce.

The rules also include output rules that set values that are pushed to the
client. These values can be used by the client to update the display of the
puzzle. These output values are part of the anti-cheating mechanism since the
true puzzle state is hidden on the server and only these outputs are available
to the clients. This also helps with bandwidth use since the server can send
small amounts of information that can cause a lot to happen on the client.

## Content Addressable Memory
Each asset required is stored in Message Pack format and is identified by a
hash of its contents. This means that every asset has a true name and can query
its peers for the asset and validate the asset returned.

This also allows the federated network to function a bit like a BitTorrent
swarm: servers can swap assets and discover other peers to spread the load.

## Capabilities
A federated network will be impossible to upgrade in a coordinated way. In the
spirit of openness, people should be able to create new experimental features
and test them in the wild without harming the network.

To accomplish this, features are associated with a _capability_. Each
capability is just a string with the name of a feature. When a realm is
created, all the capabilities it uses are written into the realm's description.
When a server gets a realm description, it can check if it has the capability
set needed to load this realm. The client can do the same. This allows a new
features to be added to an experimental server and client, be used and have
those assets distributed to incompatible servers without causing a problem.

Servers still need to process all assets in a realm, so a server must support
all the capabilities required by a realm even if the effects are mostly client
side. A client accessing a realm on a remote server should be able to use
capabilities its local server doesn't support since the local server doesn't
need to read the assets directly.
