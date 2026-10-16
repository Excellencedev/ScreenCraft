# Third-party notices

ScreenCraft itself is licensed under the [MIT License](LICENSE). The application bundles, links to,
or downloads the third-party components listed below. Each is the property of its respective authors
and is used under its own license. Nothing here changes the terms under which those components are
offered.

This file is the authoritative list. The same text is published at
<https://screencraft.mdnlabs.dev/third-party-notices>.

---

## Codecs and media libraries

- **whisper.cpp** (speech recognition, via `whisper-rs`): MIT, Copyright (c) 2023-2024 The ggml authors. The OpenAI Whisper models it runs are MIT, Copyright (c) 2022 OpenAI; they are downloaded by the user on request and never bundled.
- **Fonts**: no fonts are bundled. The interface and the text drawn on videos use the operating system's own fonts at run time.
- **H.264**: ScreenCraft bundles no H.264 codec. Encoding and decoding use the operating system's licensed implementation (VideoToolbox on macOS, Media Foundation on Windows).
- **rav1e** (AV1 encoder): **BSD-2-Clause** with the Alliance for Open Media patent licence,
  Copyright (c) The rav1e contributors. <https://github.com/xiph/rav1e>
- **libopus** (audio encoder / decoder, via `audiopus`): **BSD-3-Clause**, Copyright (c) Xiph.Org
  Foundation and contributors. <https://opus-codec.org>
- **libvpx** (VP9 encoder, only in builds made with `--features vp9`): **BSD-3-Clause** with the WebM
  patent grant, Copyright (c) The WebM Project authors. <https://www.webmproject.org>

## UI and graphics

- **egui / eframe / egui-wgpu / egui-winit**: **MIT OR Apache-2.0**, <https://github.com/emilk/egui>.
  The default fonts egui ships (Ubuntu-Light, Hack, NotoEmoji, emoji-icon-font) are used under their
  own licences (Ubuntu Font Licence 1.0, MIT, SIL OFL 1.1, OFL).
- **wgpu / naga**: **MIT OR Apache-2.0**, <https://github.com/gfx-rs/wgpu>
- **tiny-skia** (cursor artwork rasteriser): **BSD-3-Clause**, <https://github.com/linebender/tiny-skia>
- **winit**: **Apache-2.0**, <https://github.com/rust-windowing/winit>
- **cpal** (microphone and loopback streams): **Apache-2.0**, <https://github.com/RustAudio/cpal>
- **PipeWire / ashpd** (Linux screen capture through the desktop portal): PipeWire is **MIT**
  (<https://pipewire.org>, linked dynamically from the system); `ashpd` is **MIT**.

## Operating-system frameworks

ScreenCaptureKit, VideoToolbox, CoreAudio, CoreMedia (macOS), DXGI Desktop Duplication, Direct3D 11, WASAPI
(Windows) and PipeWire / xdg-desktop-portal (Linux) are system components used through their public
APIs. They are not redistributed.

## Assets created for ScreenCraft

The application icon, procedural wallpapers and the vector cursor artwork were created for this project
and are covered by the MIT License. No third-party artwork, fonts (beyond egui's defaults, above) or audio
samples are bundled.

---

## Rust crates

Resolved from `Cargo.lock` (all targets, including build tooling) by `scripts/gen-notices.py`. Grouped by
license identifier. Where a crate is offered under several licenses, ScreenCraft elects the most
permissive option (MIT or Apache-2.0).

<!-- crates:start -->
- **MIT OR Apache-2.0** (285): accesskit 0.24.1, accesskit_atspi_common 0.18.1, accesskit_consumer 0.35.0, accesskit_consumer 0.36.0, accesskit_consumer 0.38.0, accesskit_macos 0.26.3, accesskit_unix 0.21.1, accesskit_windows 0.32.1, ahash 0.8.12, aligned 0.4.3, allocator-api2 0.2.21, android-activity 0.6.1, android_system_properties 0.1.6, annotate-snippets 0.11.5, anstyle 1.0.14, anyhow 1.0.104, apple-cf 0.11.0, apple-metal 0.10.0, arbitrary 1.5.0, arboard 3.6.1, arrayvec 0.7.8, as-raw-xcb-connection 1.0.1, as-slice 0.2.1, ash 0.38.0+1.3.281, async-broadcast 0.7.2, async-recursion 1.2.0, async-trait 0.1.92, base64 0.22.1, bitflags 1.3.2, bitflags 2.13.2, bitstream-io 4.10.0, bumpalo 3.20.3, cc 1.8.0, cfg-expr 0.20.10, cfg-if 0.1.10, cfg-if 1.0.5, cmake 0.1.58, cocoa-foundation 0.2.1, core-foundation 0.10.1, core-foundation 0.9.4, core-foundation-sys 0.8.7, core-graphics 0.23.2, core-graphics 0.25.0, core-graphics-types 0.1.3, core-graphics-types 0.2.0, core_detect 1.0.0, coreaudio-rs 0.14.2, crc32fast 1.5.2, crossbeam-channel 0.5.17, crossbeam-deque 0.8.8, crossbeam-epoch 0.9.21, crossbeam-queue 0.3.14, crossbeam-utils 0.8.23, dasp_sample 0.11.0, data-url 0.3.2, dirs 7.0.0, dirs-sys 0.5.0, displaydoc 0.2.7, document-features 0.2.12, doom-fish-utils 0.4.1, downcast-rs 1.2.1, ecolor 0.36.2, eframe 0.36.2, egui 0.36.2, egui-wgpu 0.36.2, egui-winit 0.36.2, egui_extras 0.36.2, egui_glow 0.36.2, either 1.19.0, emath 0.36.2, enum-map 2.7.3, enum-map-derive 0.17.0, enumflags2 0.7.12, enumflags2_derive 0.7.12, epaint 0.36.2, errno 0.3.14, euclid 0.22.14, fdeflate 0.3.7, find-msvc-tools 0.1.14, flate2 1.1.10, font-types 0.12.6, foreign-types 0.3.2, foreign-types 0.5.0, foreign-types-macros 0.2.4, foreign-types-shared 0.1.1, foreign-types-shared 0.3.1, form_urlencoded 1.2.2, futures-core 0.3.34, futures-io 0.3.34, futures-macro 0.3.34, futures-sink 0.3.34, futures-task 0.3.34, futures-util 0.3.34, getrandom 0.2.17, getrandom 0.3.4, getrandom 0.4.3, gif 0.13.3, gif 0.14.2, glob 0.3.4, gpu-allocator 0.28.0, guillotiere 0.7.0, half 2.7.1, hashbrown 0.16.1, hashbrown 0.17.1, heck 0.5.0, hermit-abi 0.5.3, hex 0.4.3, home 0.5.12, idna 1.1.0, image 0.25.10, image-webp 0.2.4, itertools 0.13.0, itertools 0.14.0, itertools 0.15.0, itoa 1.0.18, jni 0.21.1, jni 0.22.4, jni-macros 0.22.4, jni-sys 0.3.1, jni-sys 0.4.1, jni-sys-macros 0.4.1, jobserver 0.1.35, js-sys 0.3.106, keyboard-types 0.7.0, khronos-egl 6.0.0, lazy_static 1.5.1, lazycell 1.3.0, libc 0.2.190, litrs 1.0.1, lock_api 0.4.14, log 0.4.34, memmap2 0.9.11, metal 0.18.0, minimal-lexical 0.2.1, naga 30.0.1, naga-types 30.0.1, ndk 0.9.0, ndk-context 0.1.1, ndk-sys 0.6.0+11769913, num-bigint 0.4.8, num-derive 0.4.2, num-integer 0.1.47, num-rational 0.4.2, num-traits 0.2.19, once_cell 1.21.4, ordered-stream 0.2.0, parking_lot 0.12.5, parking_lot_core 0.9.12, paste 1.0.15, pastey 0.1.1, percent-encoding 2.3.2, piper 0.2.5, pkg-config 0.3.34, plain 0.2.3, png 0.17.16, png 0.18.1, polycool 0.4.0, ppv-lite86 0.2.21, presser 0.3.1, prettyplease 0.2.37, proc-macro-crate 3.5.0, proc-macro2 1.0.107, profiling 1.0.18, profiling-procmacros 1.0.18, quick-error 2.0.1, quote 1.0.47, rand 0.9.5, rand_chacha 0.9.0, rand_core 0.9.5, range-alloc 0.1.5, raw-window-metal 1.1.0, rayon 1.12.0, rayon-core 1.13.0, read-fonts 0.41.0, regex 1.13.1, regex-automata 0.4.18, regex-syntax 0.8.11, renderdoc-sys 1.1.0, roxmltree 0.20.0, rustc_version 0.4.1, rustls-pki-types 1.15.1, rustversion 1.0.23, scoped-tls 1.0.1, scopeguard 1.2.0, screencapturekit 11.0.0, semver 1.0.28, serde 1.0.229, serde_core 1.0.229, serde_derive 1.0.229, serde_json 1.0.154, serde_repr 0.1.21, serde_spanned 1.1.2, shlex 1.3.0, shlex 2.0.1, signal-hook-registry 1.4.8, simdutf8 0.1.5, siphasher 1.0.4, skrifa 0.44.0, smallvec 1.16.3, smol_str 0.2.2, stable_deref_trait 1.2.1, static_assertions 1.1.0, syn 2.0.119, syn 3.0.8, system-deps 7.0.8, tempfile 3.27.0, thiserror 1.0.69, thiserror 2.0.21, thiserror-impl 1.0.69, thiserror-impl 2.0.21, toml 1.1.8+spec-1.1.0, toml_datetime 1.1.2+spec-1.1.0, toml_edit 0.25.17+spec-1.1.0, toml_parser 1.1.5+spec-1.1.0, toml_writer 1.1.3+spec-1.1.0, type-map 0.5.1, unicode-segmentation 1.13.3, unicode-width 0.2.2, ureq 2.12.1, url 2.5.8, version_check 0.9.5, wasm-bindgen 0.2.129, wasm-bindgen-futures 0.4.79, wasm-bindgen-macro 0.2.129, wasm-bindgen-macro-support 0.2.129, wasm-bindgen-shared 0.2.129, web-sys 0.3.106, web-time 1.1.0, weezl 0.1.12, wgpu 30.0.1, wgpu-core 30.0.1, wgpu-core-deps-apple 30.0.1, wgpu-core-deps-emscripten 30.0.1, wgpu-core-deps-wasm 30.0.1, wgpu-core-deps-windows-linux-android 30.0.1, wgpu-hal 30.0.1, wgpu-naga-bridge 30.0.1, wgpu-types 30.0.1, winapi 0.3.9, winapi-i686-pc-windows-gnu 0.4.0, winapi-x86_64-pc-windows-gnu 0.4.0, windows 0.62.2, windows-collections 0.3.2, windows-core 0.62.2, windows-future 0.3.2, windows-implement 0.60.2, windows-interface 0.59.3, windows-link 0.2.1, windows-numerics 0.3.1, windows-result 0.4.1, windows-strings 0.5.1, windows-sys 0.45.0, windows-sys 0.52.0, windows-sys 0.59.0, windows-sys 0.60.2, windows-sys 0.61.2, windows-targets 0.42.2, windows-targets 0.52.6, windows-targets 0.53.5, windows-threading 0.2.1, windows_aarch64_gnullvm 0.42.2, windows_aarch64_gnullvm 0.52.6, windows_aarch64_gnullvm 0.53.1, windows_aarch64_msvc 0.42.2, windows_aarch64_msvc 0.52.6, windows_aarch64_msvc 0.53.1, windows_i686_gnu 0.42.2, windows_i686_gnu 0.52.6, windows_i686_gnu 0.53.1, windows_i686_gnullvm 0.52.6, windows_i686_gnullvm 0.53.1, windows_i686_msvc 0.42.2, windows_i686_msvc 0.52.6, windows_i686_msvc 0.53.1, windows_x86_64_gnu 0.42.2, windows_x86_64_gnu 0.52.6, windows_x86_64_gnu 0.53.1, windows_x86_64_gnullvm 0.42.2, windows_x86_64_gnullvm 0.52.6, windows_x86_64_gnullvm 0.53.1, windows_x86_64_msvc 0.42.2, windows_x86_64_msvc 0.52.6, windows_x86_64_msvc 0.53.1, x11rb 0.13.2, x11rb-protocol 0.13.2
- **MIT** (133): aligned-vec 0.6.4, alsa-sys 0.4.0, android-properties 0.2.2, arg_enum_proc_macro 0.3.4, ashpd 0.13.13, av-scenechange 0.14.1, block 0.1.6, block2 0.5.1, block2 0.6.2, built 0.8.1, bytes 1.12.1, calloop 0.13.0, calloop 0.14.5, calloop-wayland-source 0.3.0, calloop-wayland-source 0.4.1, cfg_aliases 0.2.2, color_quant 1.1.0, combine 4.6.8, cookie-factory 0.3.3, core-media-sys 0.1.2, core-video-sys 0.1.4, crunchy 0.2.4, dispatch 0.2.0, dlib 0.5.3, endi 1.1.1, equator 0.4.2, equator-macro 0.4.2, extended 0.1.0, fax 0.2.7, float-cmp 0.9.0, fs_extra 1.3.0, glutin-winit 0.5.0, harfrust 0.12.0, imagesize 0.13.0, interpolate_name 0.2.4, libm 0.2.16, libredox 0.1.25, libspa 0.10.1, libspa-sys 0.10.1, malloc_buf 0.0.6, maybe-rayon 0.1.1, memoffset 0.9.1, new_debug_unreachable 1.0.6, nom 7.1.3, nom 8.0.0, noop_proc_macro 0.3.0, objc 0.2.7, objc-sys 0.3.5, objc2 0.5.3, objc2 0.6.5, objc2-app-kit 0.2.2, objc2-cloud-kit 0.2.2, objc2-contacts 0.2.2, objc2-core-data 0.2.2, objc2-core-image 0.2.2, objc2-core-location 0.2.2, objc2-encode 4.1.0, objc2-foundation 0.2.2, objc2-foundation 0.3.2, objc2-link-presentation 0.2.2, objc2-metal 0.2.2, objc2-quartz-core 0.2.2, objc2-symbols 0.2.2, objc2-ui-kit 0.2.2, objc2-uniform-type-identifiers 0.2.2, objc2-user-notifications 0.2.2, objc_exception 0.1.2, orbclient 0.3.55, ordered-float 5.5.0, phf 0.13.1, phf_generator 0.13.1, phf_macros 0.13.1, phf_shared 0.13.1, pico-args 0.5.0, pipewire 0.10.1, pipewire-sys 0.10.1, quick-xml 0.41.0, rdev 0.5.3, redox_syscall 0.4.1, redox_syscall 0.5.18, redox_syscall 0.9.4, redox_users 0.5.3, rfd 0.17.2, rgb 0.8.53, simd-adler32 0.3.10, simd_helpers 0.1.0, slab 0.4.12, smithay-client-toolkit 0.19.2, smithay-client-toolkit 0.20.0, smithay-clipboard 0.7.3, spin 0.9.9, strict-num 0.1.1, synstructure 0.14.0, tiff 0.11.3, tokio 1.53.2, tracing 0.1.44, tracing-attributes 0.1.31, tracing-core 0.1.36, uds_windows 1.2.1, v4l 0.14.0, v4l2-sys-mit 0.3.0, version-compare 0.2.1, wayland-backend 0.3.17, wayland-client 0.31.15, wayland-csd-frame 0.3.0, wayland-cursor 0.31.14, wayland-protocols 0.32.13, wayland-protocols-experimental 20250721.0.1, wayland-protocols-misc 0.3.12, wayland-protocols-plasma 0.3.12, wayland-protocols-wlr 0.3.12, wayland-scanner 0.31.11, wayland-sys 0.31.11, which 4.4.2, winnow 1.0.4, x11 2.21.0, x11-dl 2.21.0, xcursor 0.3.11, xkbcommon-dl 0.4.2, xml-rs 0.8.29, xmlwriter 0.1.0, y4m 0.8.0, zbus 5.19.0, zbus-lockstep 0.5.2, zbus-lockstep-macros 0.5.2, zbus_macros 5.19.0, zbus_names 4.3.4, zbus_xml 5.2.1, zcheapstr 1.1.0, zmij 1.0.23, zvariant 5.15.0, zvariant_derive 5.15.0, zvariant_utils 4.2.0
- **Apache-2.0 OR MIT** (60): alsa 0.11.0, async-channel 2.5.0, async-executor 1.14.0, async-io 2.6.0, async-lock 3.4.2, async-process 2.5.0, async-signal 0.2.14, async-task 4.7.1, atomic-waker 1.1.2, atspi 0.29.0, atspi-common 0.13.0, atspi-proxies 0.13.0, autocfg 1.5.1, bit-set 0.10.0, bit-vec 0.9.1, blocking 1.7.0, cesu8 1.1.0, cexpr 0.6.0, color 0.3.3, concurrent-queue 2.5.0, equivalent 1.0.2, event-listener 5.4.2, event-listener-strategy 0.5.4, fastrand 2.5.0, fearless_simd 0.4.1, flume 0.11.1, futures-lite 2.6.1, glifo 0.2.0, global-hotkey 0.8.0, idna_adapter 1.2.2, indexmap 2.14.2, kurbo 0.11.3, kurbo 0.13.1, linebender_resource_handle 0.1.1, multiversion_no_op 1.0.0, no_std_io2 0.9.4, nohash-hasher 0.2.0, parking 2.2.1, peeking_take_while 0.1.2, peniko 0.6.1, pin-project 1.1.13, pin-project-internal 1.1.13, pin-project-lite 0.2.17, polling 3.11.0, pollster 0.4.0, pollster 1.0.1, portable-atomic 1.15.0, portable-atomic-util 0.2.8, resvg 0.45.1, rustc-hash 1.1.0, rustc-hash 2.1.3, simd_cesu8 1.2.0, simplecss 0.2.2, svgtypes 0.15.3, usvg 0.45.1, utf8_iter 1.0.4, uuid 1.28.0, vello_common 0.1.0, vello_cpu 0.1.0, zeroize 1.9.1
- **Apache-2.0** (19): accesskit_winit 0.32.2, clang-sys 1.9.1, codespan-reporting 0.13.1, cpal 0.17.3, gethostname 1.1.0, gl_generator 0.14.0, glutin 0.32.3, glutin_egl_sys 0.7.1, glutin_glx_sys 0.6.1, glutin_wgl_sys 0.6.1, khronos_api 3.1.0, nokhwa 0.10.11, nokhwa-bindings-linux 0.1.4, nokhwa-bindings-macos 0.2.4, nokhwa-bindings-windows 0.4.6, nokhwa-core 0.1.9, spirv 0.4.0+sdk-1.4.341.0, unicode-general-category 1.1.0, winit 0.30.13
- **Zlib OR Apache-2.0 OR MIT** (19): bytemuck 1.25.2, bytemuck_derive 1.12.1, dispatch2 0.3.1, objc2-app-kit 0.3.2, objc2-audio-toolbox 0.3.2, objc2-avf-audio 0.3.2, objc2-cloud-kit 0.3.2, objc2-core-audio 0.3.2, objc2-core-audio-types 0.3.2, objc2-core-data 0.3.2, objc2-core-foundation 0.3.2, objc2-core-graphics 0.3.2, objc2-core-image 0.3.2, objc2-core-text 0.3.2, objc2-core-video 0.3.2, objc2-io-surface 0.3.2, objc2-metal 0.3.2, objc2-quartz-core 0.3.2, objc2-ui-kit 0.3.2
- **Unicode-3.0** (18): icu_collections 2.3.0, icu_locale_core 2.3.0, icu_normalizer 2.3.0, icu_normalizer_data 2.3.0, icu_properties 2.3.0, icu_properties_data 2.3.0, icu_provider 2.3.1, litemap 0.8.3, potential_utf 0.1.6, tinystr 0.8.4, writeable 0.6.4, yoke 0.8.3, yoke-derive 0.8.4, zerofrom 0.1.8, zerofrom-derive 0.1.8, zerotrie 0.2.5, zerovec 0.11.8, zerovec-derive 0.11.6
- **MPL-2.0** (11): option-ext 0.2.0, symphonia 0.5.5, symphonia-bundle-flac 0.5.5, symphonia-bundle-mp3 0.5.5, symphonia-codec-pcm 0.5.5, symphonia-codec-vorbis 0.5.5, symphonia-core 0.5.5, symphonia-format-ogg 0.5.5, symphonia-format-riff 0.5.5, symphonia-metadata 0.5.5, symphonia-utils-xiph 0.5.5
- **Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT** (7): linux-raw-sys 0.12.1, linux-raw-sys 0.4.15, rustix 0.38.44, rustix 1.1.5, wasi 0.11.1+wasi-snapshot-preview1, wasip2 1.0.4+wasi-0.2.12, wit-bindgen 0.57.1
- **MIT  OR  Apache-2.0** (7): cgl 0.3.2, cocoa 0.20.2, cocoa 0.22.0, core-foundation 0.7.0, core-foundation-sys 0.7.0, core-graphics 0.19.2, core-graphics 0.21.0
- **Unlicense OR MIT** (7): aho-corasick 1.1.5, byteorder-lite 0.1.0, memchr 2.8.3, same-file 1.0.6, termcolor 1.4.1, walkdir 2.5.0, winapi-util 0.1.11
- **MIT OR Apache-2.0 OR Zlib** (6): cursor-icon 1.2.0, glow 0.17.0, raw-window-handle 0.6.2, xkeysym 0.2.1, zune-core 0.5.3, zune-jpeg 0.5.15
- **BSD-3-Clause** (5): bindgen 0.65.1, bindgen 0.72.1, subtle 2.6.1, tiny-skia 0.11.4, tiny-skia-path 0.11.4
- **ISC** (5): audiopus 0.3.0-rc.0, audiopus_sys 0.2.2, libloading 0.8.9, rustls-webpki 0.103.15, untrusted 0.9.0
- **BSD-2-Clause** (4): arrayref 0.3.9, av1-grain 0.2.5, rav1e 0.8.1, v_frame 0.3.9
- **Zlib** (4): foldhash 0.2.0, nanorand 0.7.0, slotmap 1.1.1, zlib-rs 0.6.8
- **BSD-2-Clause OR Apache-2.0 OR MIT** (2): zerocopy 0.8.63, zerocopy-derive 0.8.63
- **BSD-3-Clause OR Apache-2.0** (2): moxcms 0.8.1, pxfm 0.1.30
- **BSD-3-Clause OR MIT OR Apache-2.0** (2): num_enum 0.7.6, num_enum_derive 0.7.6
- **BSL-1.0** (2): clipboard-win 5.4.1, error-code 3.4.0
- **CDLA-Permissive-2.0** (2): webpki-roots 0.26.11, webpki-roots 1.0.9
- **MIT OR Apache-2.0 OR LGPL-2.1-or-later** (2): r-efi 5.3.0, r-efi 6.0.0
- **MIT OR Zlib OR Apache-2.0** (2): miniz_oxide 0.8.9, miniz_oxide 0.9.1
- **Unlicense** (2): whisper-rs 0.16.0, whisper-rs-sys 0.15.0
- **(Apache-2.0 OR MIT) AND BSD-3-Clause** (1): encoding_rs 0.8.42
- **(MIT OR Apache-2.0) AND NCSA** (1): libfuzzer-sys 0.4.13
- **(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0** (1): epaint_default_fonts 0.36.2
- **(MIT OR Apache-2.0) AND Unicode-3.0** (1): unicode-ident 1.0.27
- **0BSD OR MIT OR Apache-2.0** (1): adler2 2.0.1
- **Apache-2.0 AND ISC** (1): ring 0.17.14
- **Apache-2.0 AND MIT** (1): dpi 0.1.2
- **Apache-2.0 OR GPL-2.0-only** (1): self_cell 1.3.0
- **Apache-2.0 OR ISC OR MIT** (1): rustls 0.23.46
- **Apache-2.0 WITH LLVM-exception** (1): target-lexicon 0.13.5
- **BSD-2-Clause OR MIT OR Apache-2.0** (1): mach2 0.5.0
<!-- crates:end -->

---

## License texts

### MIT License (ScreenCraft itself, egui, wgpu and others)

Each work above carries its own copyright line as stated in its section. The permission notice is:

> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

### Apache License 2.0, MPL-2.0, ISC, BSD, Zlib, Unicode-3.0, Unlicense, CDLA-Permissive-2.0

Full texts: <https://spdx.org/licenses/>. The `Cargo.lock` file pins the exact versions listed above, and
every crate's license file ships in its published source on crates.io.

---

To report an omission, or to request the corresponding source of any component, open an issue at
<https://github.com/MarshallNickolauson/ScreenCraft/issues>.
