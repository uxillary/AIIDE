# Sprite Workshop

A local browser utility for manually extracting transparent PNG frames from a sprite sheet. It does not connect to AiiDE's Tauri app or upload image data.

## Launch

From this directory, run `npm install --no-package-lock` and `npm run dev`, then open the local URL printed by Vite (normally `http://127.0.0.1:1421`). In the AiiDE repository, the existing root `node_modules` can satisfy the same dependencies without another install. Run `npm run lint`, `npm run typecheck`, `npm test`, and `npm run build` for checks.

## Try it

Import a PNG with the button or drop it on the canvas. Drag to create regions, drag a selected region to move it, or use its handles and numeric fields for exact bounds. Use the pan tool, Space drag, or middle drag to move the image; use the wheel or toolbar to zoom. Assign regions to the eight animation slots, set FPS, then Play or step through them. Reduced-motion settings disable automatic playback. Select a region and choose **Export PNG** to download its transparent crop. Repeat for each frame. Keep regions inside the artwork to exclude labels and frame numbers.
