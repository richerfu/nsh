# Images

- **Images**: background NSURLSession / file read → ImageIO downsampling decode at the view's pixel size → `NSCache` (purged under memory pressure);
  requests for the same key are coalesced and downloads nobody waits for are cancelled (Android: HttpURLConnection / APK assets →
  `BitmapFactory` power-of-two sampling → `LruCache`, same coalescing). During development `asset:/path` goes through the dev
  server's `/assets/`; `nsh build` copies assets to `out/assets/`, `nsh package` puts them in the app. Cache keys include the decode
  size and the blur radius. `blurRadius` is a stack blur on the decoding thread (`blur.rs`, `Blur.java`: the same algorithm, in
  pixels). `defaultSource` loads after the image request and shows until the image arrives; the image then fades in over
  `fadeDuration` (iOS: `CATransition`; Android: alpha over the placeholder; default 0 on iOS and 300ms on Android, as in React
  Native). Animated images: iOS decodes up to 600 frames into an animated `UIImage` (frames repeated by their delays' common
  divisor) played by a discrete `CAKeyframeAnimation` on `contents` (a tinted animated image shows its first frame); Android
  (API 28+) uses `ImageDecoder` → `AnimatedImageDrawable`, not cached (each view plays its own copy; no blur). `onLoad` reports the
  source's size in pixels on both platforms, whatever size the view decodes at. Downloads are shared per URI (also between
  views decoding at different sizes) and report `onProgress` at most every 100ms; `Image.getSize` / `Image.prefetch` (Rust,
  `nsh_core::runtime`) ask with `Command::ImageInfo`, are answered by the root's `ImageInfo` event (which the core
  resolves, never JS) and keep the downloaded bytes in a data cache (32 MB) until a view
  decodes them. Android `ImageBox` draws its borders above the
  image (as Core Animation's layer border).

On OHOS, scoped worker IO validates redirects and encoded/natural-size bounds,
and ArkUI owns decoding and presentation. The internal Image receives the
node's corner radii, a source-alpha-preserving tint matrix, native blur and a
VSync fade over `fadeDuration` (default zero). Reduced motion completes fades
immediately. The API 26 image fixture checks actual native pixels, dynamic
tint changes, transparent margins, blur, remount and independent surfaces.
