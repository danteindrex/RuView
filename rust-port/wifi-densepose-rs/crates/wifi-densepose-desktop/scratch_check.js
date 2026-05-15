const fs = require('fs');
const data = fs.readFileSync('ui-v2/public/observatory/assets/models/Soldier.glb');
const str = data.toString('utf8').replace(/[^a-zA-Z]/g, ' ');
const words = str.split(/\s+/);
const bones = words.filter(w => w.includes('Hips') || w.includes('Spine') || w.includes('Arm') || w.includes('Leg'));
console.log([...new Set(bones)].join(', '));
