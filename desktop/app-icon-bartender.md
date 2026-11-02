# Bartender application icon

Generated with the built-in image_gen tool. Source: `app-icon-bartender.png`.

## Final prompt

Use case: logo-brand. Asset type: square desktop application icon for Jartender, a Java server manager whose name evokes a bartender. Create one polished icon, no presentation mockup. Primary request: bartender-inspired icon with coffee in the background and a suit and necktie in the foreground. Composition: centered headless cropped formal suit bust, boldly simplified dark tailored lapels, crisp white shirt collar, a conventional long necktie (not bow tie). Behind the suit, a visible simple coffee cup with coffee and two or three coffee beans, clearly secondary, forming a cohesive emblem. Style: premium restrained macOS app icon, clean sculpted shapes, subtle depth and soft highlights, strong silhouette, few broad shapes readable at 32px. Warm coffee and cream backdrop with dark suit, harmonized sophisticated colors. Square rounded tile fills most of canvas with generous safe margin for the emblem. No face, no head, no hands, no letters, no text, no watermark, no extra objects. Output a single centered square icon.

## Export

From `desktop/`, run:

```sh
./node_modules/.bin/tauri icon app-icon-bartender.png --output src-tauri/icons-bartender
```

The bundle configuration uses the PNG, ICNS and ICO files in that directory.
