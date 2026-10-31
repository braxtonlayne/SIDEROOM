// @ts-check
// Shared types of the Land.io simulation. Plain data only, so a state can be hashed, copied and sent.

/**
 * Directions: 0 up, 1 right, 2 down, 3 left. `-1` means "no direction yet" (a fresh spawn waiting for input).
 * @typedef {0|1|2|3} Dir
 */

/**
 * @typedef {object} Config
 * @property {number} w              grid width in tiles
 * @property {number} h              grid height in tiles
 * @property {number} targetHeads    bots fill the arena up to this many heads (humans count first)
 * @property {number} tickHz         ticks per second; a head moves one tile per tick
 * @property {number} botRespawnTicks a bot slot waits this many ticks before a new bot spawns
 * @property {number} spawnClearance no head or trail within this many tiles (Chebyshev) of a new base centre
 * @property {boolean} cards          kills draft power-up cards (shared/cards.js)
 * @property {boolean} sandbox        practice: every card equally likely, a draft on every spawn and every 15 s
 * @property {number} spawnShieldTicks a fresh spawn cannot be cut, crashed into or blown up for this long (0 = off)
 * @property {string} map            map id (shared/maps): the terrain the arena is built on
 * @property {string} mode           mode id (shared/modes): how rounds run and are won
 * @property {'easy'|'normal'|'hard'} botSkill  how well bots play
 * @property {number} roundTicks     round length set by the host (0 = the mode's own); only for modes with rounds
 */

/**
 * One player's line in a round's results.
 * @typedef {object} ResultLine
 * @property {number} id
 * @property {string} name
 * @property {boolean} bot
 * @property {number} look
 * @property {number} team
 * @property {number} score
 * @property {boolean} alive
 */

/**
 * @typedef {object} RoundResult
 * @property {ResultLine[]} players   best first
 * @property {{team: number, score: number}[]} teams   best first (empty when there are no teams)
 * @property {number} winner          the winning player's id, or minus the winning team, or 0
 */

/**
 * The round being played (D-51). Endless modes have one round that never ends.
 * @typedef {object} Round
 * @property {number} n               1, 2, ...
 * @property {'play'|'over'} phase   'over': the results show and nobody moves
 * @property {number} start           tick the round started on (the map was rebuilt then)
 * @property {number} end             tick it ends on (0 = no time limit)
 * @property {number} next            while over: the tick the next round starts on
 * @property {RoundResult|null} result while over: the results
 */

/**
 * @typedef {object} DeathInfo
 * @property {number} tick
 * @property {string} cause          'trail' | 'self' | 'head' | 'border' | 'wall' | 'enclosed' | 'landless' | 'removed'
 * @property {string|null} killer    killer's name, null for self, border, removal
 * @property {number} land           base tiles held the tick before death
 * @property {number} maxLand        most base tiles ever held in this life
 * @property {number} kills
 * @property {number} captured       tiles captured in this life
 * @property {number} ticksAlive
 */

/**
 * @typedef {object} BotMemory
 * @property {string} mode           'idle' | 'expand' | 'home' | 'attack' | 'flee'
 * @property {number[]} legs         remaining plan as [dir, tiles, dir, tiles, ...]
 * @property {number} target         tile index an attack goes for, -1 when none
 * @property {number} boldness       0..1, scales loop size
 */

/**
 * @typedef {object} Player
 * @property {number} id             1..65535, unique among present players
 * @property {'human'|'bot'} kind
 * @property {string} name
 * @property {string|null} uid       platform user id of a human, null for bots
 * @property {number} look           colour/pattern slot (see shared/look.js)
 * @property {boolean} alive
 * @property {number} x
 * @property {number} y
 * @property {number} dir            Dir or -1
 * @property {boolean} stopped
 * @property {number[]} queue        buffered turns (at most 2), applied one per tile
 * @property {number[]} trail        tile indices of the trail in walk order; the last is the head
 * @property {number} land           base tiles owned (kept in step with the grid)
 * @property {number} gold           of those, gold tiles (terrain GOLD)
 * @property {number} team           team number (1..4) in team modes, else 0
 * @property {number} maxLand
 * @property {number} kills
 * @property {number} captured
 * @property {number} spawnTick
 * @property {number[]} bbox         [minX, minY, maxX, maxY] superset of the base, or [] when none
 * @property {BotMemory|null} bot
 * @property {DeathInfo|null} death
 * @property {{items: {id: string, level: number}[], active: {id: string, level: number, ready: number} | null, passives: {id: string, level: number}[]}} cards
 * @property {Record<string, number>} fx        timed effects: name -> tick it ends on (exclusive); also card memory
 * @property {{cards: string[], until: number}[]} offers   drafts waiting for a choice, oldest first
 */

/**
 * Something a card put in the world: a thrown grenade, a snare, a wall, a mirage. `kind` is the card id.
 * @typedef {object} WorldObject
 * @property {number} id
 * @property {string} kind
 * @property {number} owner          player id
 * @property {number} x
 * @property {number} y
 * @property {number} dir
 * @property {number} until          tick it disappears on
 * @property {boolean} [dead]        set by its card to remove it early
 * @property {Record<string, number>} [data]
 */

/**
 * @typedef {object} GameEvent
 * @property {string} k              'kill' | 'capture' | 'spawn' | 'leave' | 'card' (taken) | 'use' | 'fx' (a card effect: card, x, y)
 *                                   | 'round' (phase 'start' or 'over', n) | 'settings' (name changed mode and map)
 *                                   | 'joined' (name, via: channel / code / create) | 'left' (name, why: leave / timeout / kick)
 * @property {number} [id]           the acting player
 * @property {string} [name]
 * @property {number} [v]            victim id
 * @property {string} [vn]           victim name
 * @property {string} [c]            cause of a kill
 * @property {number} [n]            tiles captured
 * @property {string} [card]
 * @property {number} [x]
 * @property {number} [y]
 * @property {string} [phase]        a card's step, e.g. a grenade 'throw' / 'boom'
 * @property {number} [from]         a tile index the effect started from (Rewind)
 * @property {number} [dir]
 * @property {string} [mode]         'settings': the new mode
 * @property {string} [map]          'settings': the new map
 * @property {string} [via]          'joined': how they came in
 * @property {string} [why]          'left': how they went
 * (k 'host': name is the new host, or with phase 'restart' the host who restarted the round)
 */

/**
 * The whole simulation. `base` and `trail` hold a player id per tile (0 = none).
 * @typedef {object} GameState
 * @property {Config} cfg
 * @property {number} tick
 * @property {number} rng            mulberry32 state
 * @property {Uint16Array} base
 * @property {Uint16Array} trail
 * @property {Uint8Array} terrain    per tile: 0 open, 1 wall, 2 gold (shared/grid.js); set by the map
 * @property {number[]} terrainDirty tiles whose terrain changed since the last takeTerrainDirty()
 * @property {Player[]} players      sorted by id
 * @property {number} humans         seated humans (spectators included), set by the server; bots fill the rest
 * @property {number} botCooldown    ticks until the next bot may spawn
 * @property {GameEvent[]} events    what happened in the last step (cleared at the start of each step)
 * @property {Uint8Array} dirtyMark
 * @property {number[]} dirty        tiles changed since the last takeDirty()
 * @property {WorldObject[]} objects
 * @property {number} nextObjectId
 * @property {string} seed           the arena's seed (each round's map is built from it and the round number)
 * @property {Round} round
 * @property {Record<string, number | number[] | string[]>} md   mode data: whatever the mode keeps (sent to clients)
 */

/**
 * One player's input for one tick.
 * @typedef {object} Input
 * @property {number} id
 * @property {number[]} [turns]      directions, in the order pressed
 * @property {boolean} [toggleStop]  space / tap
 * @property {number[]} [cards]       card actions (shared/cards.js ACT codes), in order
 */

export {};
