import cadquery as cq


def body():
    return (
        cq.Workplane("XY")
        .box(10.1, 10.1, 6.4, centered=[True, True, False])
        .edges(">Z")
        .fillet(0.25)
        .faces(">Z")
        .circle(4)
        .extrude(2)
    )


def knob():
    return (
        cq.Workplane("XY")
        .circle(11.1 / 2)
        .extrude(0.8)
        .faces(">Z")
        .circle(9.6 / 2)
        .extrude(6.7)
        .edges(">Z")
        .chamfer(1.2, 1)
    )


def leg():
    width = 0.3
    length = 2
    return (
        cq.Workplane("YZ")
        .moveTo(-1, 2)
        .lineTo(width, 2)
        .lineTo(2 * width, width)
        .lineTo(length, width)
        .lineTo(length, 0)
        .lineTo(width, 0)
        .lineTo(0, 2 - width)
        .lineTo(-1, 2 - width)
        .close()
        .extrude(0.5, both=True)
        .edges("<<Y[5]")
        .fillet(width * 2)
        .edges("<<Y[4]")
        .fillet(width)
    )


assembly = (
    cq.Assembly()
    .add(body(), color=cq.Color("black"))
    .add(leg().translate([7.62 / 2, 10.1 / 2, 0]), color=cq.Color("gray"))
    .add(leg().translate([-7.62 / 2, 10.1 / 2, 0]), color=cq.Color("gray"))
    .add(leg().translate([7.62 / 2, 10.1 / 2, 0]).mirror("XZ"), color=cq.Color("gray"))
    .add(leg().translate([-7.62 / 2, 10.1 / 2, 0]).mirror("XZ"), color=cq.Color("gray"))
    .add(knob().translate([0, 0, 8.2 - 0.8]), color=cq.Color("gray"))
)

assembly.save("../SW_MEC_5GSH9_ROUND_LED.step")
try:
    show_object(assembly)
except AttributeError:
    pass
