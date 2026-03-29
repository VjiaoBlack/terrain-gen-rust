# Demo: building placement and demolition
Output tapes/building.gif
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"

Type "cargo run --release"
Enter
Sleep 3s

# Enter build mode
Type "b"
Sleep 1s

# Cycle to farm
Type@100ms "	"
Sleep 500ms

# Move cursor and place
Type "dddd"
Sleep 500ms
Type "ss"
Sleep 500ms
Enter
Sleep 2s

# Cycle to wall
Type@100ms "	"
Sleep 500ms
Type@100ms "	"
Sleep 500ms

# Place a wall
Type "aaa"
Sleep 500ms
Enter
Sleep 2s

# Demolish it
Type "x"
Sleep 2s

# Exit build mode
Type "b"
Sleep 2s

# Quit
Type "q"
Sleep 500ms
Type "q"
