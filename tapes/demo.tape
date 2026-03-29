# Demo: settlement overview (~8 seconds)
Output tapes/demo.gif
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

# Settlement view
Sleep 3s

# Scroll to show terrain
Down
Down
Right
Right
Sleep 2s

# Overlay flash
Type "o"
Sleep 1.5s
Type "o"
Sleep 1.5s

# Quit
Hide
Type "q"
Sleep 200ms
Type "q"
