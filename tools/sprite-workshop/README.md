# Sprite Workshop

A local browser utility for extracting and aligning transparent PNG frames from a sprite sheet. It does not connect to AiiDE's Tauri app or upload image data.

## Launch

From this directory, run `npm install --no-package-lock` and `npm run dev`, then open the local URL printed by Vite (normally `http://127.0.0.1:1421`). In the AiiDE repository, the existing root `node_modules` can satisfy the same dependencies without another install. Run `npm run lint`, `npm run typecheck`, `npm test`, and `npm run build` for checks.

## Try it

Import a PNG with the button or drop it on the canvas. Drag to create regions, drag a selected region to move it, or use its handles and numeric fields for exact bounds. Use the pan tool, Space drag, or middle drag to move the image; use the wheel or toolbar to zoom. The cross on each frame row deletes that region and clears its animation slots; **Undo deletion** restores it.

Open **Frame suggestions** in the left sidebar. A configured grid uses rows, columns, gaps and independent margins; alpha islands group connected visible pixels and nearby detached effects. **Preview suggestions** overlays proposals in amber without changing saved regions. Select a proposal to edit its coordinates, then accept or reject it. On the Elma presentation sheet, start with approximate grid rows and columns and adjust margins and regions manually. Labels, opaque backgrounds and irregular layouts are not semantically understood.

Assign regions to the eight animation slots, set FPS, then Play or step through them. **Smart alignment** is enabled by default with bottom-centre anchoring. It finds all pixels whose alpha is greater than zero (including faint sparks, hearts and shadows), aligns their bounds on a shared transparent canvas, and leaves the source rectangles intact. Switch to centre anchoring or adjust each frame's X/Y anchor offsets in the inspector. Set padding or minimum canvas width/height as needed; the displayed size is the actual export size. Guides show the shared anchor only in preview. Disable smart alignment or select **Original position** to retain motion within each crop for jumps and portals. Automatic anchoring cannot distinguish intended motion, so inspect those sequences and adjust manually.

Select a region and choose **Export PNG** to download a transparent image using the same shared layout as the preview; repeat for each frame. All saved regions contribute to the shared canvas size, even when they are not assigned to an animation slot. Keep region edges inside any labels and numbers and remove unrelated regions from the current session when sizing a particular sequence. Reduced-motion settings disable automatic playback but keep frame stepping available. Regions, proposals and settings are session-only; there are no presets, ZIP downloads or sprite-strip exports yet.
