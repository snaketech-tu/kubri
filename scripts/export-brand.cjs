// node scripts/export-brand.cjs [path-to-sharp-module]
// Deterministic exports from the checked-in vector master. No external fonts.
const fs = require('node:fs');
const path = require('node:path');
const sharp = require(process.argv[2] || 'sharp');
const root = path.resolve(__dirname, '..');
(async () => {
  const dir = path.join(root, 'ui/src-tauri/icons');
  const source = fs.readFileSync(path.join(root, 'ui/src/lib/assets/logo.svg'), 'utf8');
  const icon = source.replace('<title>', '<rect width="64" height="64" rx="14" fill="#4B64F4"/><title>').replace('d="M9 9h12', 'transform="translate(5 5) scale(.84)" d="M9 9h12').replace('d="m21 25', 'transform="translate(5 5) scale(.84)" d="m21 25').replace('fill="#4B64F4"/><path', 'fill="#ffffff"/><path');
  const images = new Map();
  for (const size of [16,24,32,128,256,512,1024]) images.set(size, await sharp(Buffer.from(icon)).resize(size,size).png().toBuffer());
  for (const [size, name] of [[32,'32x32.png'],[128,'128x128.png'],[256,'128x128@2x.png'],[512,'icon.png']]) fs.writeFileSync(path.join(dir,name),images.get(size));
  // ICO supports lossless embedded PNG entries, including the 256px entry.
  const sizes=[16,24,32,128,256], header=Buffer.alloc(6+16*sizes.length);
  header.writeUInt16LE(1,2); header.writeUInt16LE(sizes.length,4);
  let offset=header.length;
  sizes.forEach((size,i)=>{const at=6+i*16, png=images.get(size);header[at]=header[at+1]=size===256?0:size;header.writeUInt16LE(1,at+4);header.writeUInt16LE(32,at+6);header.writeUInt32LE(png.length,at+8);header.writeUInt32LE(offset,at+12);offset+=png.length;});
  fs.writeFileSync(path.join(dir,'icon.ico'),Buffer.concat([header,...sizes.map(s=>images.get(s))]));
  const chunks=[[128,'ic07'],[256,'ic08'],[512,'ic09'],[1024,'ic10']].map(([size,type])=>{const png=images.get(size),h=Buffer.alloc(8);h.write(type);h.writeUInt32BE(png.length+8,4);return Buffer.concat([h,png]);});
  const icns=Buffer.alloc(8);icns.write('icns');icns.writeUInt32BE(8+chunks.reduce((n,c)=>n+c.length,0),4);
  fs.writeFileSync(path.join(dir,'icon.icns'),Buffer.concat([icns,...chunks]));
  console.log('Exported Link app icons.');
})().catch(e=>{console.error(e);process.exit(1);});
