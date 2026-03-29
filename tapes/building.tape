# Demo: build and demolish (~10 seconds)
Output tapes/building.gif
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set Shell "zsh"

Hide
Type "export PATH=$HOME/.cargo/bin:$PATH && cd ~/Projects/terrain-gen-rust && cargo run --release"
Enter
Sleep 6s
Show

# Build mode, place farm
Type "b"
Sleep 500ms
Type "ddd"
Sleep 300ms
Tab
Sleep 500ms
Enter
Sleep 1.5s

# Place wall nearby
Tab
Tab
Sleep 300ms
Type "aa"
Sleep 300ms
Enter
Sleep 1.5s

# Demolish the wall
Type "x"
Sleep 1.5s

# Exit build, speed up briefly
Type "b"
Sleep 500ms
Type "ff"
Sleep 3s

Hide
Type "f"
Type "q"
Sleep 200ms
Type "q"
