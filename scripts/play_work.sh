#!/bin/bash
# Work mode: plain text, efficient, check state at key moments
# Usage: ./scripts/play_work.sh [seed] [ticks]
SEED=${1:-42}
TICKS=${2:-500}
W=70
H=25

cargo run --release -- --play --width $W --height $H --seed $SEED --ticks $TICKS
