#!/usr/bin/env bash
set -x
mkdir -p shots
shot() { screencapture -x "shots/$1.png"; }
appearance() { perl -e "alarm 20; exec @ARGV" osascript -e "tell application \"System Events\" to tell appearance preferences to set dark mode to $1"; echo "dark mode $1: $?"; }
appearance false
open -a /Applications/Kubri.app
sleep 20
shot 01-start-system-light
perl -e "alarm 10; exec @ARGV" osascript -e 'tell application "System Events" to get {position, size} of window 1 of (first process whose name is "kubri")'
appearance true
sleep 5
shot 02-system-dark
appearance false
sleep 5
shot 03-system-light-again
# Command palette: Cmd+K, then the toggle command.
cliclick kd:cmd t:k ku:cmd w:1000
shot 04-palette
cliclick t:'dark or light' w:800 kp:return w:2000
shot 05-after-toggle-palette
cliclick kd:cmd t:k ku:cmd w:1000 t:'dark or light' w:800 kp:return w:2000
shot 06-after-second-toggle
