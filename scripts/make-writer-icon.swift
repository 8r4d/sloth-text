// Draws Sloth Writer's app icon (Sloth's sloth, peeking over an open book,
// on an ink-blue tile) as PNGs.
// Run: swift scripts/make-writer-icon.swift writer/icons
// Writes writer/icons/icon-<size>.png and writer/icons/SlothWriter.icns.
import AppKit

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "writer/icons"

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(red: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255,
            blue: CGFloat(hex & 0xff) / 255, alpha: alpha)
}

func ellipse(_ ctx: CGContext, _ cx: CGFloat, _ cy: CGFloat, _ rx: CGFloat, _ ry: CGFloat,
             _ fill: CGColor, rotate: CGFloat = 0) {
    ctx.saveGState()
    ctx.translateBy(x: cx, y: cy)
    ctx.rotate(by: rotate)
    ctx.setFillColor(fill)
    ctx.fillEllipse(in: CGRect(x: -rx, y: -ry, width: 2 * rx, height: 2 * ry))
    ctx.restoreGState()
}

/// One page of the open book: from the spine at x = 512 out to `edge`,
/// with a gentle curve along the top and bottom. `side` is -1 (left) or 1.
func page(_ side: CGFloat, top: CGFloat, bottom: CGFloat, width: CGFloat) -> CGPath {
    let p = CGMutablePath()
    let edge = 512 + side * width
    let mid = 512 + side * width / 2
    p.move(to: CGPoint(x: 512, y: top - 30))
    p.addQuadCurve(to: CGPoint(x: edge, y: top), control: CGPoint(x: mid, y: top + 22))
    p.addLine(to: CGPoint(x: edge, y: bottom))
    p.addQuadCurve(to: CGPoint(x: 512, y: bottom - 30), control: CGPoint(x: mid, y: bottom + 18))
    p.closeSubpath()
    return p
}

/// Draw at a 1024-unit design size; the context is scaled to the output.
func draw(_ ctx: CGContext) {
    // Tile: the same rounded square as Sloth's, in ink blue.
    let inset: CGFloat = 100, side: CGFloat = 1024 - 2 * inset
    let tile = CGPath(roundedRect: CGRect(x: inset, y: inset, width: side, height: side),
                      cornerWidth: 185, cornerHeight: 185, transform: nil)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: color(0x000000, 0.35))
    ctx.addPath(tile); ctx.setFillColor(color(0x2b4370)); ctx.fillPath()
    ctx.restoreGState()
    ctx.saveGState()
    ctx.addPath(tile); ctx.clip()
    let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                              colors: [color(0x4a6fa8), color(0x1d2f55)] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(gradient, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])

    // The sloth, as on Sloth's icon but a little smaller, peeking over the book.
    ellipse(ctx, 512, 610, 250, 228, color(0x8a6a4a))
    ellipse(ctx, 512, 598, 205, 172, color(0xe9d6b8))
    ellipse(ctx, 418, 622, 80, 44, color(0x4a3424), rotate: 0.42)
    ellipse(ctx, 606, 622, 80, 44, color(0x4a3424), rotate: -0.42)
    ellipse(ctx, 428, 628, 22, 22, color(0x1a120c))
    ellipse(ctx, 596, 628, 22, 22, color(0x1a120c))
    ellipse(ctx, 435, 637, 7, 7, color(0xffffff, 0.9))
    ellipse(ctx, 603, 637, 7, 7, color(0xffffff, 0.9))
    ellipse(ctx, 512, 568, 38, 25, color(0x3a281b))

    // The book: a deep red cover, then two cream pages with lines of text.
    let top: CGFloat = 520, bottom: CGFloat = 250, width: CGFloat = 330
    for s in [CGFloat(-1), 1] {
        ctx.addPath(page(s, top: top - 6, bottom: bottom - 26, width: width + 22))
        ctx.setFillColor(color(0x8c3b2f)); ctx.fillPath()
    }
    for s in [CGFloat(-1), 1] {
        ctx.addPath(page(s, top: top, bottom: bottom, width: width))
        ctx.setFillColor(color(0xf7f0e1)); ctx.fillPath()
        // Lines of writing, following the page's curve.
        ctx.setStrokeColor(color(0xc4b597)); ctx.setLineWidth(12); ctx.setLineCap(.round)
        for i in 0..<4 {
            let y = top - 62 - CGFloat(i) * 52
            let near = 512 + s * 48, far = 512 + s * (width - 46)
            ctx.move(to: CGPoint(x: near, y: y - 24))
            ctx.addQuadCurve(to: CGPoint(x: far, y: y), control: CGPoint(x: 512 + s * width / 2, y: y + 14))
            ctx.strokePath()
        }
    }
    // The spine's fold.
    ctx.setStrokeColor(color(0xb9a888)); ctx.setLineWidth(8)
    ctx.move(to: CGPoint(x: 512, y: top - 30)); ctx.addLine(to: CGPoint(x: 512, y: bottom - 30)); ctx.strokePath()

    // Paws holding the top of the pages.
    ellipse(ctx, 330, 512, 58, 36, color(0x7a5b3e), rotate: 0.15)
    ellipse(ctx, 694, 512, 58, 36, color(0x7a5b3e), rotate: -0.15)
    ctx.restoreGState()
}

for size in [16, 32, 64, 128, 256, 512, 1024] {
    let space = CGColorSpaceCreateDeviceRGB()
    let ctx = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
                        space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.scaleBy(x: CGFloat(size) / 1024, y: CGFloat(size) / 1024)
    draw(ctx)
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    let url = URL(fileURLWithPath: "\(outDir)/icon-\(size).png")
    try! rep.representation(using: .png, properties: [:])!.write(to: url)
}
// Pack them into SlothWriter.icns (used by the app bundle) with Apple's iconutil.
let iconset = "\(outDir)/SlothWriter.iconset"
try? FileManager.default.removeItem(atPath: iconset)
try! FileManager.default.createDirectory(atPath: iconset, withIntermediateDirectories: true)
for (points, scale) in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)] {
    let name = scale == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
    try! FileManager.default.copyItem(atPath: "\(outDir)/icon-\(points * scale).png", toPath: "\(iconset)/\(name)")
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset, "-o", "\(outDir)/SlothWriter.icns"]
try! iconutil.run()
iconutil.waitUntilExit()
try? FileManager.default.removeItem(atPath: iconset)
print("Wrote icons and SlothWriter.icns to \(outDir)")
