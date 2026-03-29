# Static screenshot (~3 seconds, just the game view)
Output tapes/screenshot.gif
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set Shell "zsh"

Hide
Type "export PATH=$HOME/.cargo/bin:$PATH && cd ~/Projects/terrain-gen-rust && cargo run --release -- --screenshot --width 120 --height 40 --ticks 300"
Enter
Sleep 6s
Show
Sleep 3s
