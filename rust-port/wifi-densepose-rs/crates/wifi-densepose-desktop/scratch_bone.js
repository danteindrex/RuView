const fs = require('fs');
const glb = fs.readFileSync('ui-v2/public/observatory/assets/models/Soldier.glb');
// GLB has a JSON chunk. Let's parse it manually.
const magic = glb.readUInt32LE(0);
if (magic !== 0x46546C67) { console.log('Not a GLB'); process.exit(1); }
const jsonChunkLength = glb.readUInt32LE(12);
const jsonChunkType = glb.readUInt32LE(16);
if (jsonChunkType !== 0x4E4F534A) { console.log('No JSON chunk'); process.exit(1); }
const jsonStr = glb.toString('utf8', 20, 20 + jsonChunkLength);
const json = JSON.parse(jsonStr);
const nodes = json.nodes;
const boneNames = nodes.map(n => n.name).filter(n => n && (n.includes('Arm') || n.includes('Leg') || n.includes('Hip') || n.includes('Spine') || n.includes('mixamo')));
console.log(boneNames.join(', '));
