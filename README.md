# Gameboy Tetris Learning Environment

## Tetris Rom

Name: `Tetris (W) (V1.1) [!].gb`

SHA256 hash: `0d6535aef23969c7e5af2b077acaddb4a445b3d0df7bf34c8acef07b51b015c3`

## Detecting the end of the game in TETRIS

The procedure at the following addresses are executed at the end of a game (only tested in single player):

  1. 0x6803
  2. 0x690D
  3. 0x6964

By setting a breakpoint at one of these addresses the emulator will call a breakpoint callback that you can set, which will alert you to the end of the game, at which point you can get the score and restart the game.

## Score

The score is stored as a 3-byte little endian binary coded decimal in WRAM at addresses [0xC0A0..=0xC0A2]. Each digit is stored as 4-bits.

For example, if the 3-bytes stored at [0xC0A0..=0xC0A2] are [0x73, 0x64, 0x01], then the score would be 16473.
