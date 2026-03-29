# Static screenshot of the game
Output tapes/screenshot.png
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"

Type "cargo run --release"
Enter
Sleep 4s

# Let the settlement develop a bit
Sleep 8s

Screenshot tapes/screenshot.png
