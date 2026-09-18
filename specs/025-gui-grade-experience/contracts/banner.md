# Contract: Identity mark and boot reveal

Pinned art and animation values. The art is copied byte for byte; a worker
who needs a value that is not here stops and reports.

## Wordmark, unicode and nerd-font tiers, 64 columns and wider (59 wide)

```
██╗ ██████╗ ███╗   ██╗ █████╗ ████████╗██╗██╗   ██╗███████╗
██║██╔════╝ ████╗  ██║██╔══██╗╚══██╔══╝██║██║   ██║██╔════╝
██║██║  ███╗██╔██╗ ██║███████║   ██║   ██║██║   ██║███████╗
██║██║   ██║██║╚██╗██║██╔══██║   ██║   ██║██║   ██║╚════██║
██║╚██████╔╝██║ ╚████║██║  ██║   ██║   ██║╚██████╔╝███████║
╚═╝ ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝   ╚═╝   ╚═╝ ╚═════╝ ╚══════╝
▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔
       the PostgreSQL client you can point at production
```

## Monogram, unicode and nerd-font tiers, under 64 columns

```
██╗ ██████╗
██║██╔════╝
██║██║ ███╗   ignatius
██║██║  ██║
██║╚█████╔╝
╚═╝ ╚════╝
```

## ASCII tier (48 wide; monogram tier falls back to the plain word)

```
 ___   ___  _  _    _    _____  ___  _   _  ___
|_ _| / __|| \| |  /_\  |_   _||_ _|| | | |/ __|
 | | | (_ || .` | / _ \   | |   | | | |_| |\__ \
|___| \___||_|\_|/_/ \_\  |_|  |___| \___/ |___/
------------------------------------------------
the PostgreSQL client you can point at production
```

## Reveal

- Total: 12 ticks at the existing 90 ms tick (about 1.1 s); hard cap 1.4 s.
- Sweep front: front_col = width * ease(t), ease(t) = 1 - (1 - t)^2,
  t = ticks_elapsed / 12. Cells left of the front are final; cells within
  3 columns behind the front show a decode character; cells past the
  front are blank.
- Decode characters, chosen deterministically by (col + row + tick) so
  frames are reproducible: unicode tier `░ ▒ ▓ █`; ascii tier
  `/ \ | _ - = + *`.
- The underline row fills to front_col with the meter vocabulary; the
  tagline renders only on the final frame.
- Skip: any key, or any connection outcome, sets the state to final; the
  key event still reaches its normal handler. Reduced motion starts at
  final. Plays at most once per process.
- Placement: centered in the body area of the pre-connection shell, above
  the connecting step line, which stays visible throughout.

## Colour

- Two theme-owned anchor colours (add them as gradient anchor fields on
  the theme, dark theme anchors mauve #CBA6F7 to sky #89DCEB; light and
  high-contrast anchors chosen under the existing contrast tests).
- Per-column linear RGB lerp, one precomputed ramp used by every row.
- Depths: truecolor as computed; 256 through the US2 quantizer; 16 = two
  flat bands (anchor A for the left half, anchor B for the right);
  colour off = no colour, plain text.

## README

Under the CI badge, add the ASCII-tier art (wordmark, rule and tagline
lines exactly as above) in a fenced text block, replacing the current
tagline paragraph, which moves below the block. Monochrome is deliberate:
GitHub does not colour fenced blocks. Do not add screenshots or GIFs in
this change.

## Provenance

Original composition assembled for this product from the public figlet
block and small letterform vocabularies; not a copy of another tool's
mark. The underline-as-meter and tagline lockup are part of the mark.
