# Puzzle Pieces
This describes all the bits of puzzle logic that are available to realm
authors.

## Types
Puzzle pieces can handle different kinds of information:

- empty: used mostly for events (_e.g._, player clicked a thing)
- Boolean: a true/false, yes/no, or on/off piece of information
- number: a whole number
- link: a link to another realm or spawn point in this realm
- a list of Booleans, numbers, or links

## Pieces

### Arithmetic
Does addition, subtraction, multiplication, division, remainder (modulo), or
absolute difference on two numbers.

Commands:
- `SetLeft` + number: sets the "left" value
- `SetRight` + number: sets the "right" value

Events:
- `Changed` + number: the new result of the calculation if different from the previous result

### Buffer
A buffer stores a bunch of things in order. This can be useful for holding on
to a bunch of buttons a user has pressed. Buffers can hold Booleans, numbers,
or links.

Commands:
- `Insert` + value: inserts a value into the buffer. A buffer has a maximum size, so an old value may be dropped
- `Clear` + empty: erases all values in the buffer

Events:
- `Changed` + list of values: the buffer changed and the current values stored in the buffer
- `Selected` + value: the last value inserted into the buffer
- `Cleared` + empty: the buffer was emptied

### Button
A button the players can interact with.

Commands:
- `Enable` + empty: allows the button to emit events when pressed
- `Disable` + empty: prevents the button from emitting events when pressed
- `Enable` + Boolean: allow or prevents the button from emit events when pressed

Events:
- `Sensitive` + Boolean: indicates whether the button can be used or not
- `Changed` + empty: the button was pushed

Player Interactions:
- `Click`

### Clock
A wall clock. This produces a number: the current time over a repeating cycle (think the current "minutes" on a clock).

Settings:
- `period`: the number of seconds between each tick of the clock (_e.g._, 60 seconds for a minute clock, 3600 for an hour clock)
- `max`: the maximum number of ticks until the clock wraps around (_e.g._, 60 minutes for a minute clock, 24 for an hour clock)
- `shift`: the time when the clock should read zero. This is a bit messy to think about. Say we wanted a clock that would count the days on Olympus Mons on Mars. We need to set the time of midnight at Olympus Mons on an Earth clock. This delay is always relative to the epoch time (1970-01-01 midnight). If missing, the time the realm is created will be used as "zero" o'clock.

Events:
- `Changed` + number: the clock has ticked to the next unit of time

### Comparator
Compares two values for equality, inequality, greater-than, less-than,
less-than-or-equal-to, or greater-than-or-equal-to. The must either be numbers
or Booleans. Some of these comparisons are redundant for Booleans.

Commands:
- `SetLeft` + number: sets the "left" value
- `SetRight` + number: sets the "right" value

Events:
- `Changed` + boolean: the new result of the comparison if different from the previous result

### Counter
Counts between zero and a maximum value

Settings:
- `max`: the maximum value

Commands:
- `Up` + empty: increment the value
- `Up` + number: increment the value by the number provided
- `Down` + empty: decrement the value
- `Down` + number: decrement the value by the number provided
- `Set` + empty: set the value to zero
- `Set` + number: set the value to the number provided

Events:
- `Changed` + number: the counter's value has changed
- `AtMin` + empty: the counter is at zero
- `AtMax` + empty: the counter is at its maximum value

### Holiday
Sends a signal on a certain holidays.

Events:
- `Changed` + Boolean: indicates a shift between the holiday and regular day; true if on holiday

### Index
Pulls a particular item out of a list. This can be useful with a buffer that outputs a list. It can handle Booleans, numbers, or links.

Commands:
- `Insert` + list of value: Sets the items that can be selected from in a list
- `Set` + number: Sets the item position that should be extracted from the list

Events:
- `Changed` + empty: there is no item at the index selected
- `Changed` + value: the currently selected from the list

### Index List
Like _index_, but can select multiple items at once.

Commands:
- `Insert` + list of value: Sets the items that can be selected from in a list
- `Set` + list of number: Sets the item positions that should be extracted from the list

Events:
- `Changed` + empty: there is no item at the index selected
- `Changed` + list of value: the currently selected from the list

### Logic
Performs logic of AND, OR, XOR, NAND, or NOR on two Boolean values.

Commands:
- `SetLeft` + Boolean: sets the "left" value
- `SetRight` + Boolean: sets the "right" value

Events:
- `Changed` + Boolean: the new result of the calculation if different from the previous result

### Metronome
A clock that fires an event at a regular interval.

Settings:
- `frequency`: the number of seconds between events

Commands:
- `Frequency` + number: change the frequency of the metronome

Events:
- `Cleared` + empty: the metronome has "ticked"

### Permutation
Creates a random permutation of numbers. This can be used to create resettable
locks and combinations.

Commands:
- `Set` + empty: create a new permutation of the numbers already stored in the permutation
- `Set` + number: create a random permutation of the numbers between zero and the number provided

Events:
- `Changed` + number: the permutation has changed and the first number in the permutation
- `Changed` + list of numbers: the permutation has changed and the whole set of numbers

### Proximity
A patch of ground that allows interacting with players when they walk on it.

Commands:
- `Send` + link: send the players on the proximity area to the link provided

Events:
- `Changed` + number: the number of players on this area has changed

### Radio Button
A interaction element where a player can set a number.

Settings:
- `max`: the maximum value

Commands:
- `Enable` + empty: allows the button to emit events when pressed
- `Disable` + empty: prevents the button from emitting events when pressed
- `Enable` + Boolean: allow or prevents the button from emit events when pressed
- `Set` + number: allow or prevents the button from emit events when pressed
- `Up` + empty: increment the value selected
- `Up` + number: increment the value selected by the provided value
- `Down` + empty: decrement the value selected
- `Down` + number: decrement the value selected by the provided value

Events:
- `Sensitive` + Boolean: indicates whether the button can be used or not
- `Changed` + number: the radio button was set to a new value

Player Interactions:
- `Choose` + number


### Realm Selector
Allow players to select a realm using the realm navigation interface.

Events:
- `Changed` + link: A new realm has been selected by a player

### Sink
Hold a value. This is not really useful in the puzzles directly, but can be
useful as an intermediate storage for data being send to the players.

Commands:
- `Set` + value: change the value

### Switch
An on/off switch the player can interact with.

Commands: 
- `Down` + empty: turn the switch off
- `Up` + empty: turn the switch on
- `Toggle` + empty: toggle the switch's state
- `Enable` + empty: allow the switch to be changed by players
- `Disable` + empty: prevent the switch from being changed by players
- `Set` + Boolean: set the switch's state
- `Enable` + Boolean: allow or prevent the switch from being changed by players

Events:
- `Sensitive` + Boolean: indicates whether the switch can be used or not
- `Changed` + Boolean: the switch was toggled


### Timer
A timer that counts down to zero with a particular speed.

Settings:
- `frequency`: the number of seconds between each decrease in the timer
- `initial_counter`: the initial value in the counter

Commands:
- `Frequency` + number: set the frequency of the timer
- `Set` + number: set the current value of the timer
- `Up` + empty: increment the count by one
- `Up` + number: increment the count by the value given
- `Down` + empty: decrement the count by one
- `Down` + number: decrement the count by value given

## Consequence Rules
These rules determine how the current state of the puzzle should affect the game play.

- _number-to-property_: set a numeric property that can be seen on the client
- _Boolean-to-property_: set a Boolean property that can be seen on the client
- _Boolean-to-map_: change the terrain based on a Boolean value
- _Boolean-to-map-inverted_: change the terrain based on a Boolean value
- _number-to-bool-property_: set a Boolean property by checking if a number value satisfies some condition
- _number-to-bool-map_: change the terrain based on if a number value satisfies some condition

## Propagation Rules
These rules allow different puzzle pieces to interact with each other. Each rule has:

- a sender piece
- an event type produced by the sender
- a target piece
- a target command
- a matching transformer which checks the data with the event and, if possible, produces the data to be associated with the command

The matching transformations are:

| Event Type  | Command Type     | Input Information     | Output Information                   | Notes |
|-------------|------------------|-----------------------|--------------------------------------|-------|
| *           | _Same as event_  |                       |                                      | Copy unchanged |
| *           | Empty            |                       |                                      | Discard value |
| Empty       | Boolean          |                       | A fixed Boolean                      | Insert fixed value |
| Empty       | Number           |                       | A fixed number                       | Insert fixed value |
| Empty       | List of Booleans |                       | A fixed Boolean list                 | Insert fixed value |
| Empty       | List of Numbers  |                       | A fixed number list                  | Insert fixed value |
| Empty       | Link             |                       | A realm identifier                   | Link to global realm |
| Empty       | Link             |                       | An asset identifier                  | Link to owner realm |
| Empty       | Link             |                       | A setting identifier                 | Link to realm from settings |
| Empty       | Link             |                       | A name                               | Link to spawn Point |
| Empty       | Link             |                       |                                      | Link to home realm |
| Boolean     | Empty            | A fixed Boolean value |                                      | Match if Boolean is same |
| Boolean     | Number           | A fixed Boolean value | A fixed number                       | Match if Boolean is same and emit number |
| Boolean     | List of Booleans | A fixed Boolean value | A fixed list of Booleans             | Match if Boolean is same and emit list of Booleans |
| Boolean     | List of Numbers  | A fixed Boolean       | A fixed list of numbers              | Match if Boolean is same and emit list of numbers |
| Boolean     | Boolean          |                       |                                      | Invert Boolean value |
| Number      | Empty            | A fixed number value  |                                      | Match if number meets condition is same |
| Number      | Number           | A fixed number value  | A fixed number                       | Match if number meets condition and emit number |
| Number      | List of Booleans | A fixed number value  | A fixed list of Booleans             | Match if number meets condition and emit list of Booleans |
| Number      | List of Numbers  |                       | The number of bits and the direction | Extract the bits from zero to the number specified and convert them into Booleans as a list |
