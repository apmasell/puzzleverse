# Background for Realm Writers
The primary goal of Puzzleverse is to make it easier for writers to create new
realms. Creating game assets is hard, so this document will address creating
realms two parts: the built-in functionality of creating a realm and creating a
new game asset (_e.g._, 3D models).

All assets that have been uploaded to the Puzzleverse federated network get an
identifier that is a long string of numbers and letters. An asset can be pulled
included into the development of any realm. The goal is for work done by some
artists to be reusable by others. To this end, realms are not modelled as
complete worlds, but as small pieces that are assembled and the pieces can be
reused.

Currently, all game assets need to be licensed in a way that allows others to
use them: a [Creative Commons](https://creativecommons.org/) or public domain
licences.

## Distribution, Instances, and Links
When a realm is created, the assets will be uploaded to your server and other
servers can get copies of your realm and its assets. Once a realm is available
in the network, any player can create their own instance using the realm's
asset identifier. Every realm instance that is created also has an identifier
and accessing that realm requires that server to be online.

Updating a realm is not possible. An update is essentially creating a brand new
realm and players can create an instance of the new realm.

Realms can contain references to other realms. These are called _links_ both
because that's what _Myst_ calls them and they are effectively hyperlinks.
There are there are a few kinds:

- spawn point: a different location in the same realm
- home: sends the player to their home realm. If multiple players are linked as a group, each player is sent to their individual home realm.
- global: send a player to a particular _instance_ of a realm.
- settings: like global, but the player can customise which realm without changing the realm itself
- owner: send the players to another instance of a realm using only the asset identifier; this finds or creates the instance of that asset identifier for owner of the current realm
- train-car: send the player to the next train-car when in train-car mode

This might all be a bit abstract and this design was intended as improvement of
_Myst Online: Uru Live_.

In _MOUL_, a player in a deadly situation will _panic link_ to their home realm
(called _Relto_ in _MOUL_); this is the intended function of _home_ links.

It is desirable to create common meeting spaces, so creating a link to
particular instance via a _global_ link is meant to accomplish this. This
functionality exists in _MOUL_ but in a very different design: some _Ages_
(realms) are declared to be global (_e.g._, Ae'gura). In Puzzleverse, this may
prove to be a bad idea since it is always possible for an owner to delete the
target of a link resulting in a dead link.

To combat this, it is also possible to allow the user to change the destination
realm using a _settings_ link. This gives a default, but permits the realm
owner to adjust the link if it dies or for another reason. This could be used
to create a hub with destination that change for social reasons (_e.g._,
feature a new realm every month).

Finally, owner links are meant to emulate another _MOUL_ feature. In _MOUL_,
the player's first challenge is a set of interlinked Ages (Gahreesen, Teledahn,
Eder Kemo, Eder Gira, and Kadish Tolesa). Each of these Ages has links to some
of the others and they form an interconnected puzzle. Puzzle information cannot
be shared across realms in Puzzleverse, but they can be narratively connected.
There is a subtle change to the behaviour of _MOUL_: if me and another player
are in Teledahn and use the link to Gahreesen, I will go to _my_ Gahreesen and
they will go to _their_ Gahreesen. The Puzzleverse behaviour is that we would
both go to _my_ Gahreesen because we are in _my_ Teledahn.

Train-car links work like owner-links with the only difference being that the
train car queuing system will automatically select the next realm. When
train-car realms are created, the sequence number is baked into the realm. This
means that each player will link between realms in a consistent order, though
that order is unique to each player.

## Client/Server Split and Puzzles
When creating a realm, some of the behaviour will exist on the client and some
will exist on the server. This mostly doesn't concern realm creators, but the
following should be noted:

- the state of the puzzles is hidden on the server, so no player can extract puzzle information in order to cheat. Only things that you have made visible are visible to players.
- there is a limit to how much processing the server allows for a puzzle. If the puzzle is programmed poorly, the server will cut off the puzzle.

For the puzzle cut-off, this means:

- there is an upper limit for how large and complex a puzzle can be
- puzzles shouldn't have "unstable" behaviour where they need to scan the rules many times to reach a point where there are no new states.

Puzzles are made of _pieces_, small elemental blocks for building puzzle logic.
Some pieces are embedded in the realm where a player can interact with them
(_e.g._, push a button) and some feed back into the world (_e.g._, show an
animation if a machine was turned on). Many are the logical glue of the puzzle
that are invisible to the players.

Each puzzle piece can be given _commands_ which cause the puzzle piece to
update its state. When changed, puzzle pieces emit _events_. Part of the realm
are _rules_ which tie the events from one puzzle piece to the commands of
another.

For example, there could be a button puzzle piece, a timer puzzle piece, and a
sink puzzle piece (a generic output that controls something in the realm).
Suppose the sink is connected to a light source in the realm. When the button
is pressed, it emits an event. The rules state that when that button is
pressed, the timer is set to 30 seconds. The rules also state that if the time
remaining is greater than zero, the sink should be on. This would produce a
light on a 30 second timer.

The [Puzzle Pieces](puzzle-pieces.md) guide describes all the puzzle pieces and
how they work.

One special puzzle piece is the _proximity_ puzzle piece. This puzzle piece is
connected to a patch of ground. It functions as an input (it can count the
number of players standing on it) and it can function and an output (it can
link all the players standing on it). 
