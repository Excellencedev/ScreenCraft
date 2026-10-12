// Decodes source frames off the main thread (createImageBitmap on a 5 MP JPEG costs
// tens of milliseconds, which stalled the export loop when done on the page itself).
self.onmessage = async (e) => {
  const { id, url } = e.data;
  try {
    const r = await fetch(url);
    if (!r.ok) throw new Error("frame " + r.status);
    const bmp = await createImageBitmap(await r.blob());
    self.postMessage({ id, bmp }, [bmp]);
  } catch (err) {
    self.postMessage({ id, error: String(err) });
  }
};
