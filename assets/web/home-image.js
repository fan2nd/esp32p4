import {parseGIF, decompressFrame} from "gifuct-js";
const $=id=>document.getElementById(id), canvas=$("preview"), ctx=canvas.getContext("2d");
const limit=0x400000-4096; let encoded=null, previewFrames=[], timer=0, busy=false;
function status(s){$("status").textContent=s;}
function lock(on){busy=on;$("file").disabled=on;$("size").disabled=on;$("restore").disabled=on;$("upload").disabled=on||!encoded;}
function u32(value){const b=new Uint8Array(4);new DataView(b.buffer).setUint32(0,value,true);return b;}
// Full frames become run-length RGB565, so firmware performs no GIF/LZW work.
function runs(rgba){const out=[];let last=-1,count=0;const flush=()=>{if(count)out.push(count&255,count>>8,last&255,last>>8);};for(let i=0;i<rgba.length;i+=4){const v=((rgba[i]>>3)<<11)|((rgba[i+1]>>2)<<5)|(rgba[i+2]>>3);if(v===last&&count<65535)count++;else{flush();last=v;count=1;}}flush();return new Uint8Array(out);}
async function convert(){if(busy)return;const file=$("file").files[0];if(!file)return;lock(true);clearTimeout(timer);encoded=null;previewFrames=[];try{
 if(file.size>8*1024*1024)throw Error("源文件超过 8 MiB，请先缩小。");
 status("正在转换…");const bytes=await file.arrayBuffer();const w=Number($("size").value),h=Math.round(w*326/350);canvas.width=w;canvas.height=h;
 let total=16;const blocks=[];
 const add=(source,delay)=>{ctx.fillStyle="#e8f7ff";ctx.fillRect(0,0,w,h);const scale=Math.min(w/source.width,h/source.height);const dw=source.width*scale,dh=source.height*scale;ctx.drawImage(source,(w-dw)/2,(h-dh)/2,dw,dh);const pixels=ctx.getImageData(0,0,w,h);const data=runs(pixels.data);total+=8+data.length;if(total>limit)throw Error("转换后超过 4 MiB，请选节省空间或裁短动图。");blocks.push(u32(delay),u32(data.length),data);previewFrames.push({pixels,delay});};
 const magic=new TextDecoder().decode(bytes.slice(0,6));
 if(magic==="GIF89a"||magic==="GIF87a"){
  const gif=parseGIF(bytes),frames=gif.frames.filter(f=>f.image);const sw=gif.lsd.width,sh=gif.lsd.height;
  if(!sw||!sh||sw*sh>2048*2048||frames.length>300)throw Error("GIF 最多 300 帧、画布不超过 2048×2048。");
  const source=document.createElement("canvas");source.width=sw;source.height=sh;const sc=source.getContext("2d",{willReadFrequently:true});const patch=document.createElement("canvas"),pc=patch.getContext("2d");let previous=null,snapshot=null,elapsed=0;
  for(let i=0;i<frames.length;i++){
   if(previous?.disposalType===2){const d=previous.dims;sc.clearRect(d.left,d.top,d.width,d.height);if(previous.transparentIndex===undefined){const bg=gif.gct?.[gif.lsd.backgroundColorIndex];if(bg){sc.fillStyle=`rgb(${bg.join(",")})`;sc.fillRect(d.left,d.top,d.width,d.height);}}}
   if(previous?.disposalType===3&&snapshot)sc.putImageData(snapshot,0,0);
   const descriptor=frames[i].image.descriptor;
   if(!descriptor.width||!descriptor.height||descriptor.left+descriptor.width>sw||descriptor.top+descriptor.height>sh)throw Error("GIF 帧边界无效");
   const f=decompressFrame(frames[i],gif.gct,true),d=f.dims;
   if(d.left+d.width>sw||d.top+d.height>sh)throw Error("GIF 帧边界无效");
   snapshot=f.disposalType===3?sc.getImageData(0,0,sw,sh):null;
   patch.width=d.width;patch.height=d.height;pc.putImageData(new ImageData(f.patch,d.width,d.height),0,0);sc.drawImage(patch,d.left,d.top);
   elapsed+=Math.min(10000,Math.max(20,f.delay||100));
   if(elapsed>=80){add(source,Math.min(10000,elapsed));elapsed=0;}
   previous=f;
   if(i%4===0){status(`正在转换 ${i+1}/${frames.length} 帧…`);await new Promise(r=>setTimeout(r,0));}
  }
  if(elapsed){if(!previewFrames.length)add(source,80);else{const last=previewFrames.at(-1);last.delay=Math.min(10000,last.delay+elapsed);blocks[blocks.length-3]=u32(last.delay);}}
 }else{
  const bitmap=await createImageBitmap(new Blob([bytes],{type:file.type}));try{add(bitmap,100);}finally{bitmap.close();}
 }
 if(!previewFrames.length)throw Error("图片没有可播放帧");
 const header=new Uint8Array(16);header.set(new TextEncoder().encode("P4IMAGE1"));const dv=new DataView(header.buffer);dv.setUint16(8,w,true);dv.setUint16(10,h,true);dv.setUint16(12,previewFrames.length,true);encoded=new Blob([header,...blocks],{type:"application/octet-stream"});
 status(`预览已就绪：${previewFrames.length} 帧，${(total/1024).toFixed(1)} KiB。点击保存。`);
 let frame=0;const animate=()=>{const f=previewFrames[frame];ctx.putImageData(f.pixels,0,0);frame=(frame+1)%previewFrames.length;if(previewFrames.length>1)timer=setTimeout(animate,f.delay);};animate();
 }catch(e){status(e.message);encoded=null;}finally{lock(false);}}
async function save(restore){if(busy)return;lock(true);try{status("正在上传…");$("progress").value=0;const body=restore?new Blob([],{type:"application/octet-stream"}):encoded;
 const serial=await new Promise((resolve,reject)=>{const x=new XMLHttpRequest();x.open("POST",restore?"/default":"/image");x.setRequestHeader("Content-Type","application/octet-stream");x.timeout=120000;x.upload.onprogress=e=>{if(e.lengthComputable)$("progress").value=e.loaded/e.total*100;};x.onerror=()=>reject(Error("网络断开，请确认设备 IP"));x.ontimeout=()=>reject(Error("上传超时"));x.onload=()=>x.status===202?resolve(Number(x.responseText)):reject(Error(x.responseText));x.send(body);});
 status("上传完成，正在保存到 Flash…");const deadline=Date.now()+180000;
 while(Date.now()<deadline){await new Promise(r=>setTimeout(r,700));const r=await fetch("/state"),s=await r.json();if(s.completed===serial){if(!s.ok)throw Error("Flash 保存失败，上一张图片保留");$("progress").value=100;status(restore?"已恢复默认图片，重启后仍生效。":"已保存并更新主页，重启后仍生效。");return;}}
 throw Error("等待保存超时，请检查设备；不要立即断电。");
 }catch(e){status(e.message);}finally{lock(false);}}
$("file").addEventListener("change",convert);$("size").addEventListener("change",convert);$("upload").onclick=()=>save(false);$("restore").onclick=()=>save(true);
fetch("/state").then(r=>r.json()).then(s=>status(s.ready?(s.bytes?`设备在线，当前自定义图片 ${(s.bytes/1024).toFixed(1)} KiB。`:"设备在线，当前使用默认图片。"):"图片存储初始化中或不可用，请稍后刷新。")).catch(()=>status("连接失败，请检查设备 IP。"));
