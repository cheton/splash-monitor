# Raw footage for previews/demo.gif, recorded by ./scripts/record-previews.sh.
# The GIF is cut from tmp/demo.raw.mp4 (first 45 s at 1x, the rest at 9x), so a
# different cut needs no new recording.
Output tmp/demo.raw.mp4
Set FontSize 13
Set Columns 150
Set Rows 46
Set Margin 24
Set TypingSpeed 40ms
Type "./target/release/splash-monitor --interval 1"
Enter
# The dashboard repaints by cursor addressing and parks the cursor on the last
# row, so `Wait /…/` (which watches the final line) can never match.
Sleep 240s
Type "q"
Sleep 1s
