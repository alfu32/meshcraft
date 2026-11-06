# Native settings entry regression

The office tutorial exposed two problems in native settings automation: the
centre of a measured setting row could hit a numeric step button, and a field's
blur notification could clear the next field's active editing state.

[Before](before.json): typing `0.5` after `0.25` left a `0.35` grid. The old row
bounds included the label, unit hint and explanatory text. Paired fields also
collapsed enough to hide their numeric text. See the [before capture](before-0.5.png).

[After](after.json): eight alternating `0.25`/`0.5` edits pass across ribbon and
focus changes, with the Architecture scale preserved. The new
`setting-input/<label>` bounds identify the editable widget; paired fields use
the available row width. A blur clears editing state only when that field is
still active. The [quarter-unit capture](after-0.25.png) and
[half-unit capture](after-0.5.png) show the values and physical measurements.

Run:

```sh
python3 scripts/verify_tutorial_fields.py --binary target/debug/meshcraft
```

The check drives actual X input at the app's measured editor centres in a private
1920 × 1080 Xvfb/Mesa desktop. It writes an incomplete/failing report until all
eight cases finish. Captures are unaltered app images. The before binary had
already been bumped to 0.57.0 for authoring, before the settings fixes.
