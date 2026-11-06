# Tutorial 7 — owner's floor layout

yeah not ideal the script, in brief this is what i want

add a structural frame object (4 columns and beams), make it an object with a volumetric array hotspot, drag the instance hotspot to the other corner of the slab to fill in the plan with structure,
the structural frame should have 6x6 meters in the plan and 3.6 meters height.
multiply with the hotspot so that we have a trame of 18 meters deep and 42 meters wide (3x7 structural grid modules).
the middle 3 structural grid cores will host a solid core: the lifts case in the middle and 2 evacuation staircases on each side the plan layout is as follows:

standard plan

~~~text
  A   B   C   D   E   F   G   H
1 +```+```+```+```+```+```+```+  <- curtain wall north (A1-H1)
  |   |   |   |   |   |   |   |
2 +---+---+---+---+---+---+---+
  |   | K | S | L | S | M |   |
3 +---+---+---+---+---+---+---+
  |   |   |   |   |   |   |   |
4 +...+...+...+...+...+...+...+  <- curtain wall south (A4-H4)

~~~

K - kitchen and toilets groups
S - emergency staircases FR90 storage room and copy machine
M - meeting/conference room
L - lifts (2 left, 2 right)



Detail central node

~~~text
  B         C         D           E          F         H
2 +---------+- -+-----+---+- -+---+-----+- -+----+----+
  |  WC Wmn |   |  St | L1|   |L4 |  St |   | Sg | CM |
  +---------+ H |     +---+ HL+---+     | H +----+----+
  |  WC Men |   |     | L2|   |L3 |     |   | Kitchen |
3 +---------+- -+-----+---+- -+---+-----+- -+---------+

~~~

H -hallway FR30
St - evac stairs FR90
HL - hallway lift RF90
L1,2,3,4 - lifts
Sg - storage
CR - copy machine

Curtain wall also dynamic object defined by user with hotspots - repatable frame (left and right only, top is invisible) with glazing

partition walls are 1/2 units (15 cm)

side walls are plain 1 unit walls, no windows.