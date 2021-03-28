import cadquery as cq


def leds():
    depth = 3
    led = cq.Workplane("XY", origin=[0, 0, 8 - depth]).box(
        1.78, 5.08, depth, centered=[True, True, False]
    )

    leds = cq.Workplane("XY")
    for i in range(20):
        leds = leds.add(led.translate([i * 2.54 - 24.13, 0, 0]))
    return leds


def body():
    body_object = (
        cq.Workplane("XY")
        .box(50.7, 10.16, 8, centered=[True, True, False])
        .edges("<X and <Y")
        .chamfer(1)
    )

    base_thickness = 3.5
    base_thickness_upper = 5
    cutout1 = (
        cq.Workplane("XY")
        .rect(60, 10.16 - base_thickness)
        .workplane(offset=2)
        .rect(60, 10.16 - base_thickness_upper)
        .loft()
    )
    cutout2 = (
        cq.Workplane("XY")
        .rect(50.7 - base_thickness, 12)
        .workplane(offset=2)
        .rect(50.7 - base_thickness_upper, 12)
        .loft()
    )
    return body_object.cut(cutout1).cut(cutout2).cut(leds())


def pin():
    return cq.Workplane("XY").rect(0.5, 0.25).extrude(-4).rect(0.5, 0.25).extrude(5)


origin = cq.Vector(-24.13, -7.5 / 2, 0)
assembly = (
    cq.Assembly()
    .add(body().translate([0, 0, 0]).translate(-origin), color=cq.Color("white"))
    .add(leds().translate([0, 0, 0]).translate(-origin), color=cq.Color(0, 1, 0, 0.5))
)

# add pins
for i in range(20):
    assembly.add(
        pin().translate([i * 2.54 - 24.13, -7.5 / 2, 0]).translate(-origin),
        color=cq.Color("gray"),
    )
    assembly.add(
        pin().translate([i * 2.54 - 24.13, 7.5 / 2, 0]).translate(-origin),
        color=cq.Color("gray"),
    )

assembly.save("../LED_BARGRAPH_20.step")
try:
    show_object(assembly)
except AttributeError:
    pass
