import math

import cadquery as cq


def hexpoints(r: float):
    return [
        (math.cos(i * math.pi / 3) * r, math.sin(i * math.pi / 3) * r) for i in range(6)
    ]


def pinpoints(wp: cq.Workplane):
    return wp.pushPoints([(-20, i * 2.54 - 2.54 * 3.5) for i in range(8)])


def pcb():
    pcb = (
        cq.Workplane("XY")
        .box(45, 31, 1.6, centered=[True, True, False])
        .edges("|Z")
        .fillet(2.5)
        .faces(">Z")
        .rect(40, 26, forConstruction=True)
        .vertices()
        .hole(3)
    )
    return pinpoints(pcb.faces(">Z").workplane()).hole(1)


def display():
    return cq.Workplane("XY").box(23.4, 23.4, 1, centered=[True, True, False])


def spacer():
    spacer = (
        cq.Workplane("XY")
        .polyline(hexpoints(2.5))
        .close()
        .extrude(10)
        .circle(2.5 / 2)
        .cutThruAll()
        .edges(">Z or <Z")
        .fillet(0.3)
    )
    screw = (
        cq.Workplane("XY", origin=[0, 0, 11.8])
        .tag("base")
        .circle(2.5)
        .extrude(1)
        .faces(">Z")
        .fillet(0.6)
        .faces(">Z")
        .workplane()
        .polyline(hexpoints(1.2))
        .close()
        .cutBlind(-0.8)
        .workplaneFromTagged("base")
        .circle(2.5 / 2)
        .extrude(-5)
    )
    return spacer.add(screw)


assembly = (
    cq.Assembly()
    .add(pcb().translate([40 / 2, 0, 10]), color=cq.Color(0.000, 0.314, 0.549))
    .add(display().translate([24.9, 0, 11.6]), color=cq.Color("black"))
    .add(spacer().translate([40, 13, 0]), color=cq.Color("gray"))
    .add(spacer().translate([40, -13, 0]), color=cq.Color("gray"))
    .add(spacer().translate([0, 13, 0]), color=cq.Color("gray"))
    .add(spacer().translate([0, -13, 0]), color=cq.Color("gray"))
)

assembly.save("../Waveshare_LCD_240X240.step")
try:
    show_object(assembly)
except AttributeError:
    pass
