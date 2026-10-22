# Third-party notices

## Patched window backend

`vendor/winit` contains winit 0.30.13 under its Apache-2.0 license (see
`vendor/winit/LICENSE`). The Windows backend has a narrow local recovery patch:
failed `ClientToScreen`/`GetWindowRect` queries return an unavailable position,
and failed size queries return zero while window destruction is processed.
This prevents Win32 query failures from panicking across native callbacks.
Other platform implementations are unchanged. The invalid-HWND regression test
runs with `cargo test -p winit --lib desktop_query_tests`.

## Background photographs

The JPEG files under `assets/backgrounds/` were downloaded from Unsplash and are
not covered by this repository's MIT license. They are redistributed under the
[Unsplash License](https://unsplash.com/license), which permits free commercial
and non-commercial use and distribution, subject to its restrictions. Attribution
is not required by the license but is appreciated.

This project uses the photographs only as integrated UI backgrounds. It does not
sell them as standalone images or compile them to reproduce a stock-photo service.

| Local file | Original source | Contributor information retained |
| --- | --- | --- |
| `rain-0.jpg` | [Unsplash image](https://images.unsplash.com/photo-1515694346937-94d85e41e6f0) | — |
| `rain-1.jpg` | [Unsplash image](https://images.unsplash.com/photo-1534274988757-a28bf1a57c17) | — |
| `rain-2.jpg` | [Unsplash image](https://images.unsplash.com/photo-1428592953211-077101b2021b) | — |
| `meteor-0.jpg` | [Unsplash image](https://images.unsplash.com/photo-1464802686167-b939a6910659) | — |
| `meteor-1.jpg` | [Unsplash image](https://images.unsplash.com/photo-1419242902214-272b3f66ee7a) | — |
| `meteor-2.jpg` | [Unsplash image](https://images.unsplash.com/photo-1444703686981-a3abbc4d4fe3) | — |
| `blossom-0.jpg` | [Source page](https://unsplash.com/photos/h-ghoFr1D5s) | Jerry Wei |
| `blossom-1.jpg` | [Source page](https://unsplash.com/photos/im3ik4Nfb8M) | Magic Fan |
| `blossom-2.jpg` | [Source page](https://unsplash.com/photos/mqNn1qeDx2M) | youichiro ogawa |
| `snow-0.jpg` | [Unsplash image](https://images.unsplash.com/photo-1510798831971-661eb04b3739) | — |
| `snow-1.jpg` | [Unsplash image](https://images.unsplash.com/photo-1638985787285-46f05ff063f2) | — |
| `snow-2.jpg` | [Source page](https://unsplash.com/photos/5gEj_9CBhIw) | Fabian Kleiser |
| `beach-0.jpg` | [Source page](https://unsplash.com/photos/fO8-qeOHhuQ) | Barth Bailey |
| `beach-1.jpg` | [Source page](https://unsplash.com/photos/6tZUyXUFJuw) | Georgii Eletskikh |
| `beach-2.jpg` | [Source page](https://unsplash.com/photos/ktZZiHb-GoI) | MontyLov |
| `clear-0.jpg` | [Source page](https://unsplash.com/photos/WyAOGgtqVyE) | Jimmy Liu |
| `clear-1.jpg` | [Source page](https://unsplash.com/photos/KpxO2ASf9OU) | Spencer DeMera |
| `clear-2.jpg` | [Source page](https://unsplash.com/photos/cJRHha43aMU) | — |
| `cloudy-0.jpg` | [Source page](https://unsplash.com/photos/v5Mj5oR1pRM) | Yousef Espanioly |
| `cloudy-1.jpg` | [Source page](https://unsplash.com/photos/3_TuRDGfPUs) | Christian Bruno |
| `cloudy-2.jpg` | [Source page](https://unsplash.com/photos/uGbZm-3a0Js) | Lina Simonyan |
| `overcast-0.jpg` | [Source page](https://unsplash.com/photos/FT-I_VxodFs) | Felix Preiss |
| `overcast-1.jpg` | [Source page](https://unsplash.com/photos/SED8CLAHX-Y) | engin akyurt |
| `overcast-2.jpg` | [Source page](https://unsplash.com/photos/7IkwmfDl3x4) | Wolfgang Hasselmann |
| `fog-0.jpg` | [Source page](https://unsplash.com/photos/uNVd8Kl-mkE) | Will Smith |
| `fog-1.jpg` | [Source page](https://unsplash.com/photos/voUogf6jOtg) | Matt Drenth |
| `fog-2.jpg` | [Source page](https://unsplash.com/photos/qJDhugDloGg) | Will Swann |

If you own one of these photographs and would like the credit corrected or the
bundled copy removed, please open an issue.

## Weather icons

The SVG files under `assets/weather-icons/` are selected from
[QWeather Icons](https://github.com/qwd/Icons), copyright QWeather, and are
distributed under the [Creative Commons Attribution 4.0 International
License](https://creativecommons.org/licenses/by/4.0/). The bundled copies use
condition-specific fill colours for legibility on the widget's dark forecast
strip. Attribution and the license link are also included in that directory.

The generated SVG files under `assets/weather-themes/` reuse geometry from
[QWeather Icons 1.8.0](https://github.com/qwd/Icons), copyright QWeather,
under its MIT license. The local generation script changes only presentation
(palette, gradient and glow) and retains a copy of the upstream license.

## Software dependencies

Runtime and development dependencies are declared in `Cargo.toml` and resolved in
`Cargo.lock`. They retain their respective upstream licenses and are not relicensed
by this project.

## Weather data

Location search and weather forecasts are provided by
[Open-Meteo](https://open-meteo.com/). Geocoding results are based on GeoNames data;
refer to the [Open-Meteo terms](https://open-meteo.com/en/terms) for attribution,
licensing, and service conditions.

District-level location search falls back to
[OpenStreetMap Nominatim](https://nominatim.org/). Search results contain data
© OpenStreetMap contributors and are made available under the ODbL; see the
[OpenStreetMap copyright page](https://www.openstreetmap.org/copyright).
