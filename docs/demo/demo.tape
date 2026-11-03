# Records docs/demo/demo.gif and docs/demo/completion.png. Needs the compose database with
# docs/demo/schema.sql loaded and a release build:
#   cargo build --release -p oracli
#   vhs docs/demo/demo.tape

Output docs/demo/demo.gif

Set Shell bash
Set FontSize 16
Set Width 1400
Set Height 640
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 90ms

Hide
# A fresh history, so no auto-suggestions from earlier sessions.
Type "export XDG_DATA_HOME=$(mktemp -d) && clear && ./target/release/oracli scott/tiger@localhost:1521/FREEPDB1"
Enter
Sleep 4s
Show

Sleep 500ms
Type "sel"
Sleep 800ms
Tab
Sleep 400ms
Type " e.ename, d.dname, e.sal from emp e join d"
Sleep 1s
Tab
Sleep 600ms
Type " d on e."
Sleep 1.5s
Screenshot docs/demo/completion.png
Sleep 500ms
Type "dep"
Sleep 600ms
Tab
Sleep 600ms
Type " = d.dep"
Sleep 800ms
Tab
Sleep 600ms
Type " where e.s"
Sleep 1s
Down
Sleep 600ms
Type " >= 2500 order by e.sal desc;"
Sleep 500ms
Enter
Sleep 2.5s

