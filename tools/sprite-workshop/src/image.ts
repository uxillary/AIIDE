const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10]

export type SourceImage = { bitmap: ImageBitmap; name: string; width: number; height: number }

export async function loadPng(file: File): Promise<SourceImage> {
  const header = new Uint8Array(await file.slice(0, 8).arrayBuffer())
  if (!PNG_SIGNATURE.every((byte, index) => header[index] === byte)) throw new Error('Choose a valid PNG image. Other formats are not supported.')
  let bitmap: ImageBitmap
  try { bitmap = await createImageBitmap(file) }
  catch { throw new Error('This PNG could not be decoded. Try another image.') }
  if (!bitmap.width || !bitmap.height || bitmap.width > 16384 || bitmap.height > 16384 || bitmap.width * bitmap.height > 64_000_000) {
    bitmap.close()
    throw new Error('Image is too large for the workshop (maximum 16,384 px per side and 64 million pixels).')
  }
  return { bitmap, name: file.name, width: bitmap.width, height: bitmap.height }
}
