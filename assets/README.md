# Asset provenance

- `fonts/BarlowCondensed-BoldItalic.ttf`: Barlow Condensed by Jeremy Tribby, obtained from the Google Fonts repository. Used for Latin/numeric display typography. SIL Open Font License in the adjacent file.
- `fonts/NotoSans.ttf`: Noto Sans from the Google Fonts repository. Cyrillic and other unsupported display glyphs use this fallback. SIL Open Font License in the adjacent file.
- `demo-background.png`: generated with the built-in ImageGen tool for this project, 2026-09-27. It depicts a fictional park, not a user's footage or private location. It is used only for documentation and synthetic test videos, never included in transparent layers. The generated image is upscaled for the 4K *video fixture*; gauge/text/map geometry are rendered natively at the requested output size.
- All dashboard graphics are original configurable geometric drawing and font rendering. No game assets or reference screenshots are distributed.
- OSM map data is fetched separately, cached outside git, and credited on the overlay. Map data © OpenStreetMap contributors, ODbL: https://www.openstreetmap.org/copyright.

## Demo background prompt

> Generate a photorealistic synthetic demonstration background for an open-source bicycle telemetry video renderer, a wide 16:9 first-person action-camera view from a bicycle on an empty paved cycling path through a green riverside park in soft late afternoon sunlight. Camera level facing forward, distant trees, small river visible on right, fresh natural green grass, realistic color. No bicycle or hands in foreground. No people, text, logos, UI, speedometer or infographics. Quiet dark green grass/pavement textures at the lower corners for a later white overlay. A fictional landscape, no identifiable private location.

Fonts: https://github.com/google/fonts/tree/main/ofl/barlowcondensed and https://github.com/google/fonts/tree/main/ofl/notosans.
