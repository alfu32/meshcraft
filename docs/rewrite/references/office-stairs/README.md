# Office stair route regression

The native before check reopens the published 0.57.0 model and measures a 0.150 m
approach. Both flights ascend in positive Z; the return flight mesh misses the
1.8 m landing. [Measured failures](before/07-office-floor-stair-checks.json).

The core regression `drawn_line_preserves_direction_through_overlap_and_splitting`
failed before the fix: a line drawn from Z=12 to Z=0 was stored from Z=0 to Z=12.
It passes after retaining the source orientation during canonical-axis clustering,
including collinear overlap and crossing splits.

The corrected native saved-model checks and plan/junction views live in
[the tutorial review directory](../../../tutorials/review/). They check both
actual flight mesh contacts, opposite ascent, approach clearance and entrance doors.
