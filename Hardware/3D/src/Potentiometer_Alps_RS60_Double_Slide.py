import cadquery as cq


def body():
    return (
        cq.Workplane("XY", origin=[0, 0, 3.5])
        .box(75, 9, 7)
        .edges("|Z or >Z")
        .fillet(0.25)
        .edges("<Z")
        .fillet(1.5)
        # cutout
        .faces(">Z")
        .rect(60, 2)
        .cutBlind(-3)
    )


def pin():
    return (
        cq.Workplane("XY", origin=[0, 0, -3.5])
        .box(0.4, 0.6, 5, centered=[True, True, False])
    )


def hook():
    return (
        cq.Workplane("XZ")
        .moveTo(-0.2, 0)
        .line(0.4, 0)
        .line(0, -1)
        .line(-0.5, -0.5)
        .line(0.5, -1.5)
        .line(-0.4, 0)
        .line(-0.6, 1.6)
        .line(0.6, 0.6)
        .close()
        .extrude(0.25, both=True)
    )


def lever():
    return (
        cq.Workplane("XY")
        .tag("base")
        .rect(6.6, 2)
        .workplane(offset=1)
        .rect(5.5, 2)
        .loft()
        .rect(5, 1.2)
        .extrude(4)
        .faces(">Z")
        .rect(4, 1.2)
        .extrude(10)
        .workplaneFromTagged("base")
        .rect(6.6, 2)
        .extrude(-3)
    )


def knob_outer():
    ellipse = (
        cq.Workplane("XZ")
        .ellipse(25 / 2, 13.5)
        .extrude(13 / 2, both=True)
    )
    circle = (
        cq.Workplane("XZ")
        .circle(15)
        .extrude(13 / 2, both=True)
        .translate([0, 0, 20])
    )
    base_object = (
        ellipse
        .cut(circle)
        .copyWorkplane(cq.Workplane("XY"))
        .split(keepTop=True)
        .edges("<Z and >X or <X")
        .chamfer(1.5)
    )
    cutout = cq.Workplane("XZ").circle(0.4).extrude(15, both=True)
    half_object = (
        base_object
        .copyWorkplane(cq.Workplane("YZ", origin=[0.5, 0, 0]))
        .split(keepTop=True)
        # grip cutouts
        .cut(cutout.translate([2.5, 0, 5.3]))
        .cut(cutout.translate([5, 0, 5.85]))
        .cut(cutout.translate([7.5, 0, 7]))
    )
    return half_object.add(half_object.mirror("YZ"))

def knob_inner():
    return (
        cq.Workplane("XY")
        .box(1, 12, 4, centered=[True, True, False])
    )

LEVER_POS = 0.75

lever_x = -30 + 6.6 / 2 + LEVER_POS * (60 - 6.6)

origin = cq.Vector(-35, -3.75, 0)
assembly = (
    cq.Assembly()
    .add(body().translate([0, 0, 0]).translate(-origin), color=cq.Color("gray"))
    .add(pin().translate([-35, -3.75, 0]).translate(-origin), color=cq.Color("gold"))
    .add(pin().translate([-35, -1.25, 0]).translate(-origin), color=cq.Color("gold"))
    .add(pin().translate([-35, 1.25, 0]).translate(-origin), color=cq.Color("gold"))
    .add(pin().translate([-35, 3.25, 0]).translate(-origin), color=cq.Color("gold"))
    .add(pin().translate([35, -3.25, 0]).translate(-origin), color=cq.Color("gold"))
    .add(pin().translate([35, 1.25, 0]).translate(-origin), color=cq.Color("gold"))
    .add(hook().translate([-25, 0, 0]).translate(-origin), color=cq.Color("gray"))
    .add(hook().mirror("ZY").translate([22.5, 0, 0]).translate(-origin), color=cq.Color("gray"))
    .add(lever().translate([lever_x, 0, 7]).translate(-origin), color=cq.Color("gray"))
    .add(knob_outer().translate([lever_x, 0, 7 + 12]).translate(-origin), color=cq.Color("black"))
    .add(knob_inner().translate([lever_x, 0, 7 + 12]).translate(-origin), color=cq.Color("white"))
)
assembly.save("../Potentiometer_Alps_RS60_Double_Slide.step")
try:
    show_object(assembly)
except AttributeError:
    pass
