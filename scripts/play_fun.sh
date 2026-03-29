#!/bin/bash
# Fun mode: rich ANSI color output, view lots of frames, explore freely
# Usage: ./scripts/play_fun.sh [seed] [ticks]
SEED=${1:-42}
TICKS=${2:-500}
W=90
H=35

cargo run --release -- --play --width $W --height $H --seed $SEED \
  --inputs "tick:$TICKS,ansi"
