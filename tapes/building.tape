# Demo: building placement and demolition
Output tapes/building.gif
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set Shell "zsh"

Type "export PATH=$HOME/.cargo/bin:$PATH && cd ~/Projects/terrain-gen-rust && cargo run --release"
Enter
Sleep 6s

# Enter build mode
Type "b"
Sleep 1s

# Move cursor
Type "dddd"
Sleep 500ms
Type "ss"
Sleep 500ms

# Cycle to farm
Tab
Sleep 500ms

# Place farm
Enter
Sleep 2s

# Cycle to wall
Tab
Tab
Sleep 500ms

# Move and place
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

# Speed up to watch villagers
Type "f"
Type "f"
Sleep 5s

# Quit
Type "f"
Sleep 1s
Type "q"
Sleep 500ms
Type "q"
