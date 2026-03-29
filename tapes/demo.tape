# Demo: settlement founding and early gameplay
Output tapes/demo.gif
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set Shell "zsh"

Type "export PATH=$HOME/.cargo/bin:$PATH && cd ~/Projects/terrain-gen-rust && cargo run --release"
Enter
Sleep 6s

# Watch the settlement
Sleep 5s

# Scroll around
Down
Down
Down
Down
Sleep 2s
Right
Right
Right
Right
Sleep 2s

# Toggle overlay
Type "o"
Sleep 2s
Type "o"
Sleep 2s

# Speed up
Type "f"
Sleep 3s
Type "f"
Sleep 3s

# Back to normal
Type "f"
Sleep 2s

# Goto settlement
Type "g"
Sleep 2s

# Quit
Type "q"
Sleep 500ms
Type "q"
