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

# Move cursor with wasd
Type "dddd"
Sleep 500ms
Type "ss"
Sleep 500ms

# Cycle to farm with tab
Tab
Sleep 500ms

# Place farm
Enter
Sleep 2s

# Cycle to wall
Tab
Tab
Sleep 500ms

# Move and place a wall
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

# Speed up to watch villagers build
Type "f"
Type "f"
Sleep 5s

# Back to normal and quit
Type "f"
Sleep 1s
Type "q"
Sleep 500ms
Type "q"
