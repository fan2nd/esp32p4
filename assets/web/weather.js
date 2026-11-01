// Location lookup runs only on explicit search; P4 fetches weather independently.
const city = document.querySelector('#weather-city');
const results = document.querySelector('#weather-results');
const weatherStatus = document.querySelector('#weather-status');
const latitude = document.querySelector('#latitude');
const longitude = document.querySelector('#longitude');
const weatherButtons = [...document.querySelectorAll('#weather-settings button')];
let locations = [];
document.querySelector('#weather-search').onclick = async () => {
  if (city.value.trim().length < 2) { weatherStatus.textContent = '请输入至少两个字符，可使用城市拼音。'; return; }
  weatherStatus.setAttribute("aria-busy", "true");
  weatherButtons.forEach(b => b.disabled = true);
  try {
    const response = await fetch(`https://geocoding-api.open-meteo.com/v1/search?name=${encodeURIComponent(city.value.trim())}&count=10&language=en`, {signal: AbortSignal.timeout(15000)});
    if (!response.ok) throw new Error('城市搜索失败，可手动填写经纬度');
    locations = (await response.json()).results || [];
    results.replaceChildren(new Option('请选择搜索结果', ''));
    locations.forEach((p, i) => results.add(new Option([p.name, p.admin1, p.country].filter(Boolean).join(' · '), String(i))));
    weatherStatus.textContent = locations.length ? '请选择城市，再保存。' : '未找到城市，请试拼音或手动输入经纬度。';
  } catch(e) { weatherStatus.textContent = e.message; }
  finally { weatherStatus.setAttribute("aria-busy", "false"); weatherButtons.forEach(b => b.disabled = false); }
};
results.onchange = () => {
  if (results.value === '') return;
  const p = locations[Number(results.value)];
  city.value = p.name; latitude.value = p.latitude; longitude.value = p.longitude;
};
async function loadWeather() {
  const response = await fetch('/weather', {signal: AbortSignal.timeout(10000)});
  if (!response.ok) throw new Error('读取天气设置失败');
  return response.json();
}
async function saveWeather(clear) {
  const lat = Number(latitude.value), lon = Number(longitude.value), name = city.value.trim();
  if (!clear && (!name || new TextEncoder().encode(name).length > 48 || !latitude.value || !longitude.value || !Number.isFinite(lat) || !Number.isFinite(lon) || Math.abs(lat) > 90 || Math.abs(lon) > 180)) {
    weatherStatus.textContent = '请选择城市或填写有效名称及经纬度（名称最多 48 字节）。'; return;
  }
  const desired = clear ? null : {name, latitude: Math.round(lat * 1e6), longitude: Math.round(lon * 1e6)};
  weatherStatus.setAttribute("aria-busy", "true");
  weatherButtons.forEach(b => b.disabled = true);
  try {
    const response = await fetch('/weather', {method: 'POST', headers: {'Content-Type':'application/octet-stream'}, body: JSON.stringify(desired), signal: AbortSignal.timeout(10000)});
    if (!response.ok) throw new Error(await response.text());
    weatherStatus.textContent = '已应用，等待保存到 Flash…';
    const until = Date.now() + 30000;
    while (Date.now() < until) {
      await new Promise(resolve => setTimeout(resolve, 1000));
      const [saved, weather, status] = await loadWeather();
      if (JSON.stringify(saved) !== JSON.stringify(desired)) throw new Error('配置已被其他页面修改，请重新读取');
      if (status === 'Saved to flash') { weatherStatus.textContent = `已保存到 Flash。${clear ? '天气已关闭。' : weather}`; return; }
      if (status === 'KV storage unavailable') throw new Error('Flash 存储不可用，未保存');
    }
    throw new Error('保存尚未确认，请稍后重新打开页面检查。');
  } catch(e) { weatherStatus.textContent = e.message; }
  finally { weatherStatus.setAttribute("aria-busy", "false"); weatherButtons.forEach(b => b.disabled = false); }
}
document.querySelector('#weather-save').onclick = () => saveWeather(false);
document.querySelector('#weather-disable').onclick = () => saveWeather(true);
loadWeather().then(([location, weather, status]) => {
  if (location) { city.value = location.name; latitude.value = location.latitude / 1e6; longitude.value = location.longitude / 1e6; }
  weatherStatus.textContent = `${weather} · ${status}`;
}).catch(e => weatherStatus.textContent = e.message);
