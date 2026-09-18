# Sprite Workshop

A local browser utility for extracting and aligning transparent PNG frames from a sprite sheet. It does not connect to AiiDE's Tauri app or upload image data.

## Launch

From this directory, run `npm install --no-package-lock` and `npm run dev`, then open the local URL printed by Vite (normally `http://127.0.0.1:1421`). In the AiiDE repository, the existing root `node_modules` can satisfy the same dependencies without another install. Run `npm run lint`, `npm run typecheck`, `npm test`, and `npm run build` for checks.

## Try it

Import a PNG with the button or drop it on the canvas. Drag to create regions, drag a selected region to move it, or use its handles and numeric fields for exact bounds. Use the pan tool, Space drag, or middle drag to move the image; use the wheel or toolbar to zoom. The cross on each frame row deletes that region and clears its animation slots; **Undo deletion** restores it.

Open **Frame suggestions** in the left sidebar. A configured grid uses rows, columns, gaps and independent margins; alpha islands group connected visible pixels and nearby detached effects. **Preview suggestions** overlays proposals in amber without changing saved regions. Select a proposal to edit its coordinates, then accept or reject it. On the Elma presentation sheet, start with approximate grid rows and columns and adjust margins and regions manually. Labels, opaque backgrounds and irregular layouts are not semantically understood.

Assign regions to the eight animation slots, then use **Animation alignment** to place each crop on one shared transparent pixel canvas. **Align all to anchor** resets the eight slot offsets to bottom centre or centre, whichever initial anchor is selected. Drag the alignment preview, focus it and press an arrow key, use the one-pixel nudge buttons, or enter exact X/Y offsets. **Reset slot** clears only the selected slot. The crosshair marks the common anchor, and optional onion skinning overlays the previous or first slot while paused. Crop rectangles retain their original source coordinates and transparent pixels; neither alpha trimming nor per-frame recentering occurs. Use offsets to preserve deliberate motion, such as jumps, and increase padding or minimum canvas dimensions if an offset clips artwork. Set FPS, then Play or step through the slots to see the same placements.

The inspector's **Export PNG** downloads only the source crop and does not include animation offsets. **Export aligned slot PNG** downloads the active slot on the shared canvas with its offset, matching the animation preview without guides or onion skinning. Repeat for each assigned slot to save an aligned sequence. Only assigned crops contribute to canvas size. Reduced-motion settings disable automatic playback but keep frame stepping available. Regions, assignments and settings are session-only; there are no presets, ZIP downloads or sprite-strip exports yet.
