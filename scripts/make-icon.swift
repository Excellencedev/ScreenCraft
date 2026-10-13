// Renders the ScreenCraft app icon (1024x1024) with CoreGraphics.
// Usage: swift scripts/make-icon.swift src-tauri/icons/icon-1024.png
import AppKit
import CoreGraphics

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon-1024.png"
let S = 1024
let cs = CGColorSpace(name: CGColorSpace.sRGB)!
guard let ctx = CGContext(data: nil, width: S, height: S, bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { fatalError("ctx") }

func color(_ r: Double, _ g: Double, _ b: Double, _ a: Double = 1) -> CGColor {
    CGColor(colorSpace: cs, components: [r / 255, g / 255, b / 255, a])!
}

// macOS icon grid: 824pt tile centred in the 1024pt canvas
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let radius: CGFloat = 186
func squircle(_ r: CGRect, _ rad: CGFloat) -> CGPath {
    CGPath(roundedRect: r, cornerWidth: rad, cornerHeight: rad, transform: nil)
}
let shape = squircle(tile, radius)

// soft drop shadow under the tile
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -14), blur: 26, color: color(0, 20, 70, 0.28))
ctx.addPath(shape)
ctx.setFillColor(color(40, 100, 200))
ctx.fillPath()
ctx.restoreGState()

// blue gradient fill
ctx.saveGState()
ctx.addPath(shape)
ctx.clip()
let grad = CGGradient(colorsSpace: cs, colors: [color(94, 168, 240), color(38, 98, 200), color(30, 70, 168)] as CFArray,
                      locations: [0, 0.55, 1])!
ctx.drawLinearGradient(grad, start: CGPoint(x: 280, y: 924), end: CGPoint(x: 760, y: 100), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
// top sheen
let sheen = CGGradient(colorsSpace: cs, colors: [color(255, 255, 255, 0.28), color(255, 255, 255, 0)] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(sheen, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 560), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
ctx.restoreGState()

// thin inner edge
ctx.saveGState()
ctx.addPath(squircle(tile.insetBy(dx: 1.5, dy: 1.5), radius - 1.5))
ctx.setStrokeColor(color(255, 255, 255, 0.35))
ctx.setLineWidth(3)
ctx.strokePath()
ctx.restoreGState()

// the mark: record target (ring, ring, dot) with a soft shadow, as in the original logo
let c = CGPoint(x: 512, y: 512)
func ring(_ r: CGFloat, _ w: CGFloat) {
    ctx.addArc(center: c, radius: r, startAngle: 0, endAngle: .pi * 2, clockwise: false)
    ctx.setLineWidth(w)
    ctx.strokePath()
}
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 22, color: color(8, 30, 90, 0.35))
ctx.setStrokeColor(color(255, 255, 255))
ring(214, 64)
ctx.setFillColor(color(255, 255, 255))
ctx.addArc(center: c, radius: 78, startAngle: 0, endAngle: .pi * 2, clockwise: false)
ctx.fillPath()
ctx.restoreGState()

// four crop corners hint at "screen" without cluttering the mark
ctx.saveGState()
ctx.setStrokeColor(color(255, 255, 255, 0.55))
ctx.setLineWidth(26)
ctx.setLineCap(.round)
ctx.setLineJoin(.round)
let o: CGFloat = 288, l: CGFloat = 70
for (sx, sy) in [(-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] as [(CGFloat, CGFloat)] {
    let p = CGPoint(x: c.x + sx * o, y: c.y + sy * o)
    ctx.move(to: CGPoint(x: p.x - sx * l, y: p.y))
    ctx.addLine(to: p)
    ctx.addLine(to: CGPoint(x: p.x, y: p.y - sy * l))
}
ctx.strokePath()
ctx.restoreGState()

let img = ctx.makeImage()!
let rep = NSBitmapImageRep(cgImage: img)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
