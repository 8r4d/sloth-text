// Draws Sloth's app icon (a sloth face on a leafy green tile) as PNGs.
// Run: swift scripts/make-icon.swift icons
// Writes icons/icon-<size>.png and icons/Sloth.icns.
import AppKit

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icons"

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

/// Draw at a 1024-unit design size; the context is scaled to the output.
func draw(_ ctx: CGContext) {
    // Tile: macOS-style rounded square with a soft vertical gradient.
    let inset: CGFloat = 100, side: CGFloat = 1024 - 2 * inset
    let tile = CGPath(roundedRect: CGRect(x: inset, y: inset, width: side, height: side),
                      cornerWidth: 185, cornerHeight: 185, transform: nil)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: color(0x000000, 0.35))
    ctx.addPath(tile); ctx.setFillColor(color(0x4f8a4b)); ctx.fillPath()
    ctx.restoreGState()
    ctx.saveGState()
    ctx.addPath(tile); ctx.clip()
    let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                              colors: [color(0x7cbf6a), color(0x3f7a45)] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(gradient, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
    // A branch along the bottom.
    ctx.setStrokeColor(color(0x6b4a2b)); ctx.setLineWidth(56); ctx.setLineCap(.round)
    ctx.move(to: CGPoint(x: 60, y: 250)); ctx.addQuadCurve(to: CGPoint(x: 980, y: 300), control: CGPoint(x: 520, y: 170))
    ctx.strokePath()
    ctx.restoreGState()

    // Head (shaggy brown) and face (light tan).
    ellipse(ctx, 512, 540, 300, 270, color(0x8a6a4a))
    ellipse(ctx, 512, 525, 245, 205, color(0xe9d6b8))
    // Dark eye patches that slope down and out, the classic sloth look.
    ellipse(ctx, 400, 545, 95, 52, color(0x4a3424), rotate: 0.42)
    ellipse(ctx, 624, 545, 95, 52, color(0x4a3424), rotate: -0.42)
    // Sleepy eyes.
    ellipse(ctx, 412, 552, 26, 26, color(0x1a120c))
    ellipse(ctx, 612, 552, 26, 26, color(0x1a120c))
    ellipse(ctx, 420, 562, 8, 8, color(0xffffff, 0.9))
    ellipse(ctx, 620, 562, 8, 8, color(0xffffff, 0.9))
    // Nose and a contented smile.
    ellipse(ctx, 512, 470, 46, 30, color(0x3a281b))
    ctx.setStrokeColor(color(0x3a281b)); ctx.setLineWidth(14); ctx.setLineCap(.round)
    ctx.move(to: CGPoint(x: 452, y: 410)); ctx.addQuadCurve(to: CGPoint(x: 572, y: 410), control: CGPoint(x: 512, y: 370))
    ctx.strokePath()
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
// Pack them into Sloth.icns (used by the app bundle) with Apple's iconutil.
let iconset = "\(outDir)/Sloth.iconset"
try? FileManager.default.removeItem(atPath: iconset)
try! FileManager.default.createDirectory(atPath: iconset, withIntermediateDirectories: true)
for (points, scale) in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)] {
    let name = scale == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
    try! FileManager.default.copyItem(atPath: "\(outDir)/icon-\(points * scale).png", toPath: "\(iconset)/\(name)")
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset, "-o", "\(outDir)/Sloth.icns"]
try! iconutil.run()
iconutil.waitUntilExit()
try? FileManager.default.removeItem(atPath: iconset)
print("Wrote icons and Sloth.icns to \(outDir)")
