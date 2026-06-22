// ============================================================================
// Bedrock geometry (.geo.json) -> THREE.Group  (algoritmo CANONICO)
// Verificado contra el fuente real de Blockbench (rama master):
//   - js/util/three_custom.js      -> setShape (orden EXACTO de 24 vertices)
//   - js/preview/canvas.js         -> face_order ['east','west','up','down','south','north']
//   - js/outliner/types/cube.js    -> updateUV box_uv layout + FLOOR de w,h,d
//   - js/formats/bedrock/bedrock.js-> mirror X de handedness al importar
//
// Reemplaza en preview.html: boxUVrects, FACE_INDEX, applyBoxUV, applyPerFaceUV,
// buildCube, buildModel.  En render():  current = geoToThreeGroup(geoRoot, tex);
// Requiere THREE r128 (el que ya usas).
// ============================================================================
'use strict';
const deg = d => d * Math.PI / 180;
const FACE_ORDER = ['east', 'west', 'up', 'down', 'south', 'north'];

// 8 -> 24 vertices, EXACTAMENTE en el orden de Blockbench (setShape).
function setShape(p, f, t) {
  p.set([t[0],t[1],t[2], t[0],t[1],f[2], t[0],f[1],t[2], t[0],f[1],f[2]], 0);   // east
  p.set([f[0],t[1],f[2], f[0],t[1],t[2], f[0],f[1],f[2], f[0],f[1],t[2]], 12);  // west
  p.set([f[0],t[1],f[2], t[0],t[1],f[2], f[0],t[1],t[2], t[0],t[1],t[2]], 24);  // up
  p.set([f[0],f[1],t[2], t[0],f[1],t[2], f[0],f[1],f[2], t[0],f[1],f[2]], 36);  // down
  p.set([f[0],t[1],t[2], t[0],t[1],t[2], f[0],f[1],t[2], t[0],f[1],t[2]], 48);  // south
  p.set([t[0],t[1],f[2], f[0],t[1],f[2], t[0],f[1],f[2], f[0],f[1],f[2]], 60);  // north
}

// Layout box-uv canonico. size puede ser NEGATIVO (= flip), NO lo normalices.
function boxUVRects(u, v, w, h, d) {
  const R = (fx, fy, sx, sy) => [u + fx, v + fy, u + fx + sx, v + fy + sy];
  return {
    east:  R(0,        d, d,  h),
    west:  R(d + w,    d, d,  h),
    up:    R(d + w,    d, -w, -d),
    down:  R(d + 2*w,  0, -w, d),
    south: R(2*d + w,  d, w,  h),
    north: R(d,        d, w,  h),
  };
}

function rotateUVquad(arr, rot) {
  rot = ((rot || 0) % 360 + 360) % 360;
  while (rot > 0) { const a = arr[0]; arr[0] = arr[2]; arr[2] = arr[3]; arr[3] = arr[1]; arr[1] = a; rot -= 90; }
  return arr;
}

// V volteada (1 - v/TH) porque usamos texture.flipY=false.
function writeFaceUV(uvAttr, faceIdx, rect, TW, TH, faceRot) {
  const u0 = rect[0] / TW, v0 = rect[1] / TH, u1 = rect[2] / TW, v1 = rect[3] / TH;
  let q = [[u0, 1 - v0], [u1, 1 - v0], [u0, 1 - v1], [u1, 1 - v1]];
  q = rotateUVquad(q, faceRot);
  const b = faceIdx * 4;
  uvAttr.setXY(b + 0, q[0][0], q[0][1]); uvAttr.setXY(b + 1, q[1][0], q[1][1]);
  uvAttr.setXY(b + 2, q[2][0], q[2][1]); uvAttr.setXY(b + 3, q[3][0], q[3][1]);
}

function buildCubeGeometry(cube, TW, TH) {
  const size = cube.size || [0, 0, 0], origin = cube.origin || [0, 0, 0], inflate = cube.inflate || 0;
  // mirror X (handedness Bedrock -> Three.js): origin es esquina MIN
  let from = [-(origin[0] + size[0]), origin[1], origin[2]];
  let to   = [from[0] + size[0], from[1] + size[1], from[2] + size[2]];
  for (let i = 0; i < 3; i++) { from[i] -= inflate; to[i] += inflate; if (from[i] === to[i]) to[i] += 0.001; }

  const geom = new THREE.BufferGeometry();
  const pos = new Float32Array(72); setShape(pos, from, to);
  geom.setAttribute('position', new THREE.BufferAttribute(pos, 3));
  geom.setAttribute('uv', new THREE.BufferAttribute(new Float32Array(48), 2));
  const idx = []; for (let fc = 0; fc < 6; fc++) { const b = fc * 4; idx.push(b, b + 2, b + 1, b + 2, b + 3, b + 1); }
  geom.setIndex(idx);

  const uvAttr = geom.attributes.uv;
  if (Array.isArray(cube.uv)) { // BOX UV
    const w = Math.floor(Math.abs(size[0])), h = Math.floor(Math.abs(size[1])), d = Math.floor(Math.abs(size[2]));
    let rects = boxUVRects(cube.uv[0], cube.uv[1], w, h, d);
    if (cube.mirror) { for (const k in rects) { const r = rects[k]; rects[k] = [r[2], r[1], r[0], r[3]]; } }
    FACE_ORDER.forEach((fk, i) => writeFaceUV(uvAttr, i, rects[fk], TW, TH, 0));
  } else if (cube.uv && typeof cube.uv === 'object') { // PER-FACE UV
    FACE_ORDER.forEach((fk, i) => {
      const f = cube.uv[fk];
      if (!f || !f.uv) { writeFaceUV(uvAttr, i, [0, 0, 0, 0], TW, TH, 0); return; }
      const ux = f.uv[0], uy = f.uv[1], uw = (f.uv_size ? f.uv_size[0] : 0), uh = (f.uv_size ? f.uv_size[1] : 0);
      let rect = [ux, uy, ux + uw, uy + uh];          // uv_size negativo = flip (no corregir)
      if (fk === 'up' || fk === 'down') rect = [rect[0], rect[3], rect[2], rect[1]]; // flip vertical
      writeFaceUV(uvAttr, i, rect, TW, TH, f.uv_rotation || 0);
    });
  }
  geom.computeVertexNormals();
  return geom;
}

function geoToThreeGroup(geoRoot, texture) {
  const geo = (geoRoot['minecraft:geometry'] || [])[0];
  if (!geo) throw new Error('geo.json sin minecraft:geometry');
  const desc = geo.description || {}, TW = desc.texture_width || 16, TH = desc.texture_height || 16;
  if (texture) {
    texture.magFilter = THREE.NearestFilter; texture.minFilter = THREE.NearestFilter;
    texture.generateMipmaps = false; texture.needsUpdate = true;  // flipY por defecto (true) + uv=1-v = correcto (no doble flip)
  }
  const mat = new THREE.MeshLambertMaterial({ map: texture, side: THREE.DoubleSide, transparent: true, alphaTest: 0.5 });
  const root = new THREE.Group(), groups = {}, bones = geo.bones || [];
  const pivotOf = b => { const p = (b.pivot || [0, 0, 0]); return [-p[0], p[1], p[2]]; }; // mirror X

  for (const b of bones) { const g = new THREE.Group(); g.name = b.name; g.userData.bone = b; groups[b.name] = g; }
  for (const b of bones) {
    const g = groups[b.name];
    const parent = (b.parent && groups[b.parent]) ? groups[b.parent] : root;
    const piv = pivotOf(b);
    const ppiv = (b.parent && groups[b.parent]) ? pivotOf(groups[b.parent].userData.bone) : [0, 0, 0];
    g.position.set(piv[0] - ppiv[0], piv[1] - ppiv[1], piv[2] - ppiv[2]);
    g.rotation.order = 'ZYX';
    if (b.rotation) { // negar X,Y de rotacion (handedness); Z igual; orden ZYX
      g.rotation.set(deg(-b.rotation[0]), deg(-b.rotation[1]), deg(b.rotation[2]));
    }
    // pose de REPOSO (para que la animación se aplique ENCIMA, no la pise)
    g.userData.rest = { px: g.position.x, py: g.position.y, pz: g.position.z, rx: g.rotation.x, ry: g.rotation.y, rz: g.rotation.z };
    parent.add(g);

    for (const c of (b.cubes || [])) {
      let geom; try { geom = buildCubeGeometry(c, TW, TH); } catch (e) { continue; }
      const mesh = new THREE.Mesh(geom, mat);
      if (c.rotation) { // rotacion de cubo alrededor de su pivot
        const cpRaw = c.pivot || [(c.origin ? c.origin[0] : 0), 0, 0];
        const cpiv = [-cpRaw[0], cpRaw[1] || 0, cpRaw[2] || 0]; // mirror X
        const sub = new THREE.Group(); sub.rotation.order = 'ZYX';
        sub.position.set(cpiv[0] - piv[0], cpiv[1] - piv[1], cpiv[2] - piv[2]);
        sub.rotation.set(deg(-c.rotation[0]), deg(-c.rotation[1]), deg(c.rotation[2]));
        mesh.position.set(-cpiv[0], -cpiv[1], -cpiv[2]);
        sub.add(mesh); g.add(sub);
      } else {
        // geom esta en coords de MODELO (ya con X negada) -> a local del bone
        mesh.position.set(-piv[0], -piv[1], -piv[2]);
        g.add(mesh);
      }
    }
  }
  return root;
}

// Exporta para usar desde preview.html (o copia/pega las funciones directamente).
if (typeof window !== 'undefined') window.geoToThreeGroup = geoToThreeGroup;
