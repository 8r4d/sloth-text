// Draws Sloth Writer's sloth on its own (the one on the app icon, without
// its tile), in three poses, as transparent PNGs for the app's empty
// states, start screen and celebrations:
//   reading:  peeking over an open book, as on the icon
//   asleep:   eyes shut, head resting on a closed book, a few z's
//   cheering: happy, arms up
// Run: swift scripts/make-writer-sloth.swift writer/icons
// Writes writer/icons/sloth-<pose>.png (512 × 512).
import AppKit

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "writer/icons"

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(red: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255,
            blue: CGFloat(hex & 0xff) / 255, alpha: alpha)
}

// The icon's colours.
let fur = color(0x8a6a4a), face = color(0xe9d6b8), patch = color(0x4a3424)
let eye = color(0x1a120c), nose = color(0x3a281b), paw = color(0x7a5b3e)
let cover = color(0x8c3b2f), paper = color(0xf7f0e1), ink = color(0xc4b597), fold = color(0xb9a888)

func ellipse(_ ctx: CGContext, _ cx: CGFloat, _ cy: CGFloat, _ rx: CGFloat, _ ry: CGFloat,
             _ fill: CGColor, rotate: CGFloat = 0) {
    ctx.saveGState()
    ctx.translateBy(x: cx, y: cy)
    ctx.rotate(by: rotate)
    ctx.setFillColor(fill)
    ctx.fillEllipse(in: CGRect(x: -rx, y: -ry, width: 2 * rx, height: 2 * ry))
    ctx.restoreGState()
}

func stroke(_ ctx: CGContext, _ width: CGFloat, _ c: CGColor, _ build: (CGMutablePath) -> Void) {
    let p = CGMutablePath()
    build(p)
    ctx.addPath(p)
    ctx.setStrokeColor(c); ctx.setLineWidth(width); ctx.setLineCap(.round); ctx.setLineJoin(.round)
    ctx.strokePath()
}

enum Eyes { case open, shut, happy }

/// The sloth's head, centred at (cx, cy), as on the icon (whose head is
/// centred at 512, 610).
func head(_ ctx: CGContext, _ cx: CGFloat, _ cy: CGFloat, eyes: Eyes, smile: Bool) {
    let dx = cx - 512, dy = cy - 610
    func at(_ x: CGFloat, _ y: CGFloat) -> (CGFloat, CGFloat) { (x + dx, y + dy) }
    var (x, y) = at(512, 610); ellipse(ctx, x, y, 250, 228, fur)
    (x, y) = at(512, 598); ellipse(ctx, x, y, 205, 172, face)
    (x, y) = at(418, 622); ellipse(ctx, x, y, 80, 44, patch, rotate: 0.42)
    (x, y) = at(606, 622); ellipse(ctx, x, y, 80, 44, patch, rotate: -0.42)
    for side in [CGFloat(-1), 1] {
        let (ex, ey) = at(512 + side * 84, 628)
        switch eyes {
        case .open:
            ellipse(ctx, ex, ey, 22, 22, eye)
            ellipse(ctx, ex + 7, ey + 9, 7, 7, color(0xffffff, 0.9))
        case .shut:
            // A gentle downward curve: asleep.
            stroke(ctx, 9, eye) { p in
                p.move(to: CGPoint(x: ex - 24, y: ey + 4))
                p.addQuadCurve(to: CGPoint(x: ex + 24, y: ey + 4), control: CGPoint(x: ex, y: ey - 16))
            }
        case .happy:
            // An upward arc: smiling eyes.
            stroke(ctx, 10, eye) { p in
                p.move(to: CGPoint(x: ex - 24, y: ey - 8))
                p.addQuadCurve(to: CGPoint(x: ex + 24, y: ey - 8), control: CGPoint(x: ex, y: ey + 24))
            }
        }
    }
    (x, y) = at(512, 568); ellipse(ctx, x, y, 38, 25, nose)
    if smile {
        let (mx, my) = at(512, 528)
        stroke(ctx, 9, nose) { p in
            p.move(to: CGPoint(x: mx - 44, y: my + 6))
            p.addQuadCurve(to: CGPoint(x: mx + 44, y: my + 6), control: CGPoint(x: mx, y: my - 30))
        }
    }
}

/// One page of the open book, as on the icon.
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

func openBook(_ ctx: CGContext) {
    let top: CGFloat = 520, bottom: CGFloat = 250, width: CGFloat = 330
    for s in [CGFloat(-1), 1] {
        ctx.addPath(page(s, top: top - 6, bottom: bottom - 26, width: width + 22))
        ctx.setFillColor(cover); ctx.fillPath()
    }
    for s in [CGFloat(-1), 1] {
        ctx.addPath(page(s, top: top, bottom: bottom, width: width))
        ctx.setFillColor(paper); ctx.fillPath()
        for i in 0..<4 {
            let y = top - 62 - CGFloat(i) * 52
            stroke(ctx, 12, ink) { p in
                p.move(to: CGPoint(x: 512 + s * 48, y: y - 24))
                p.addQuadCurve(to: CGPoint(x: 512 + s * (width - 46), y: y), control: CGPoint(x: 512 + s * width / 2, y: y + 14))
            }
        }
    }
    stroke(ctx, 8, fold) { p in
        p.move(to: CGPoint(x: 512, y: top - 30)); p.addLine(to: CGPoint(x: 512, y: bottom - 30))
    }
}

/// A long sloth arm from `from` to `to`, with three claws at the end.
func arm(_ ctx: CGContext, from: CGPoint, to: CGPoint) {
    stroke(ctx, 78, fur) { p in p.move(to: from); p.addLine(to: to) }
    let angle = atan2(to.y - from.y, to.x - from.x)
    for spread in [CGFloat(-0.45), 0, 0.45] {
        let a = angle + spread
        stroke(ctx, 13, face) { p in
            p.move(to: CGPoint(x: to.x + cos(a) * 26, y: to.y + sin(a) * 26))
            p.addLine(to: CGPoint(x: to.x + cos(a) * 66, y: to.y + sin(a) * 66))
        }
    }
}

func reading(_ ctx: CGContext) {
    head(ctx, 512, 610, eyes: .open, smile: false)
    openBook(ctx)
    ellipse(ctx, 330, 512, 58, 36, paw, rotate: 0.15)
    ellipse(ctx, 694, 512, 58, 36, paw, rotate: -0.15)
}

func asleep(_ ctx: CGContext) {
    // A closed book to lie on: its cover, and the edge of its pages.
    let book = CGRect(x: 170, y: 170, width: 684, height: 150)
    ctx.addPath(CGPath(roundedRect: book, cornerWidth: 26, cornerHeight: 26, transform: nil))
    ctx.setFillColor(cover); ctx.fillPath()
    ctx.addPath(CGPath(roundedRect: book.insetBy(dx: 26, dy: 28).offsetBy(dx: 14, dy: 0), cornerWidth: 10, cornerHeight: 10, transform: nil))
    ctx.setFillColor(paper); ctx.fillPath()
    for i in 0..<3 {
        let y = book.minY + 54 + CGFloat(i) * 22
        stroke(ctx, 5, ink) { p in p.move(to: CGPoint(x: book.minX + 60, y: y)); p.addLine(to: CGPoint(x: book.maxX - 40, y: y)) }
    }
    // The head, tipped over and resting on it.
    ctx.saveGState()
    ctx.translateBy(x: 470, y: 470); ctx.rotate(by: 0.28); ctx.translateBy(x: -470, y: -470)
    head(ctx, 470, 520, eyes: .shut, smile: false)
    ctx.restoreGState()
    // Paws over the edge of the book.
    ellipse(ctx, 300, 330, 62, 36, paw, rotate: 0.1)
    ellipse(ctx, 640, 322, 62, 36, paw, rotate: -0.05)
    // z z z, getting bigger as they float up.
    for (i, (x, y, s)) in [(CGFloat(735), CGFloat(650), CGFloat(36)), (800, 735, 50), (880, 840, 66)].enumerated() {
        stroke(ctx, 10 + CGFloat(i) * 2, color(0x6f87b8)) { p in
            p.move(to: CGPoint(x: x - s / 2, y: y + s / 2))
            p.addLine(to: CGPoint(x: x + s / 2, y: y + s / 2))
            p.addLine(to: CGPoint(x: x - s / 2, y: y - s / 2))
            p.addLine(to: CGPoint(x: x + s / 2, y: y - s / 2))
        }
    }
}

func cheering(_ ctx: CGContext) {
    // Arms up behind the head, claws open.
    arm(ctx, from: CGPoint(x: 360, y: 430), to: CGPoint(x: 190, y: 800))
    arm(ctx, from: CGPoint(x: 664, y: 430), to: CGPoint(x: 834, y: 800))
    // A round body below.
    ellipse(ctx, 512, 300, 210, 170, fur)
    ellipse(ctx, 512, 290, 140, 120, color(0xa3825f))
    head(ctx, 512, 560, eyes: .happy, smile: true)
}

for (name, pose) in [("reading", reading), ("asleep", asleep), ("cheering", cheering)] as [(String, (CGContext) -> Void)] {
    let size = 512
    let ctx = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.scaleBy(x: CGFloat(size) / 1024, y: CGFloat(size) / 1024)
    pose(ctx)
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: "\(outDir)/sloth-\(name).png"))
}
print("Wrote sloth-reading.png, sloth-asleep.png and sloth-cheering.png to \(outDir)")
