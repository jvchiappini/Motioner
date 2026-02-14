(function(){
  const filesUrl = './_files.json';
  const filesEl = document.getElementById('files');
  const reader = document.getElementById('reader');
  const titleEl = document.getElementById('currentTitle');
  const search = document.getElementById('search');
  const openInEditor = document.getElementById('openInEditor');

  let files = [];
  let current = null;

  function renderSidebar(list){
    filesEl.innerHTML = '';
    // group by top-level folder (e.g., elements/, components/)
    const groups = {};
    list.forEach(f => {
      const parts = f.path.split('/');
      const group = parts.length > 1 ? parts[0] : '_root';
      groups[group] = groups[group] || [];
      groups[group].push(f);
    });

    // render groups (root first)
    const orderedKeys = Object.keys(groups).sort((a,b)=> (a === '_root') ? -1 : (b === '_root') ? 1 : a.localeCompare(b));
    orderedKeys.forEach(key => {
      if (key === '_root') {
        groups[key].forEach(f => {
          const div = document.createElement('div');
          div.className = 'file-item';
          div.dataset.path = f.path;
          div.innerHTML = `<div class=\"name\">${f.title}</div><div class=\"meta\">${f.desc || f.path}</div>`;
          div.addEventListener('click', () => loadFile(f));
          filesEl.appendChild(div);
        });
        return;
      }

      const groupEl = document.createElement('div');
      groupEl.className = 'sidebar-group';
      const title = document.createElement('div');
      title.className = 'group-title';
      title.textContent = key.charAt(0).toUpperCase() + key.slice(1);
      title.addEventListener('click', () => groupEl.classList.toggle('collapsed'));
      groupEl.appendChild(title);

      const listEl = document.createElement('div');
      listEl.className = 'group-list';
      groups[key].forEach(f => {
        const sub = document.createElement('div');
        sub.className = 'file-item sub-item';
        sub.dataset.path = f.path;
        sub.innerHTML = `<div class=\"name\">${f.title}</div><div class=\"meta\">${f.desc || f.path}</div>`;
        sub.addEventListener('click', () => loadFile(f));
        listEl.appendChild(sub);
      });
      groupEl.appendChild(listEl);
      filesEl.appendChild(groupEl);
    });
  }

  function slugify(s){
    // defensivo: aceptar tokens/objects y convertir a string
    try {
      s = (s === null || s === undefined) ? '' : String(s);
    } catch (e) {
      s = '';
    }
    return s.toLowerCase().replace(/[^a-z0-9]+/g,'-').replace(/(^-|-$)/g,'');
  }

  function renderTOC(tokens){
    const headings = tokens.filter(t => t.type === 'heading' && (t.depth===2 || t.depth===3));
    if(!headings.length) return '';
    let html = '<nav class="doc-toc"><strong>En esta página</strong><ul>';
    headings.forEach(h => {
      // marked token heading.text may be a string or an array of tokens — normalize safely
      let text = '';
      if (typeof h.text === 'string') text = h.text;
      else if (h.tokens && Array.isArray(h.tokens)) {
        text = h.tokens.map(t => (t.text || t.raw || '')).join('');
      } else if (h.raw) {
        text = h.raw;
      } else {
        text = String(h);
      }
      const id = slugify(text);
      html += `<li class="toc-h${h.depth}"><a href="#${id}">${text}</a></li>`;
    });
    html += '</ul></nav>';
    return html;
  }

  async function loadFile(f){
    try{
      const res = await fetch(f.path);
      if(!res.ok) throw new Error('No se pudo cargar');
      let md = await res.text();

      // --- Preprocess custom shortcodes ---
      // color: {color:#hex_or_name}text{/color}
      md = md.replace(/\{color:([#a-zA-Z0-9\-()%,.]+)\}([\s\S]*?)\{\/color\}/g, (_, color, inner) => {
        return `<span class="md-color" style="color:${color}">${inner}</span>`;
      });
      // background: {bg:#hex}text{/bg}
      md = md.replace(/\{bg:([#a-zA-Z0-9\-()%,.]+)\}([\s\S]*?)\{\/bg\}/g, (_, color, inner) => {
        return `<span class="md-bg" style="background:${color};padding:2px 6px;border-radius:4px">${inner}</span>`;
      });
      // badge: [!badge:TEXT]
      md = md.replace(/\[!badge:([^\]]+)\]/g, (_, txt) => ` <span class="md-badge">${txt}</span> `);

      // produce tokens to build a TOC and first-paragraph summary
      const tokens = marked.lexer(md, {gfm:true});
      const tocHtml = renderTOC(tokens);

      // natural-language summary: first non-empty paragraph (normalize token text)
      const firstParaToken = tokens.find(t => t.type === 'paragraph');
      let summary = '';
      if (firstParaToken) {
        if (typeof firstParaToken.text === 'string') summary = firstParaToken.text;
        else if (firstParaToken.tokens && Array.isArray(firstParaToken.tokens)) {
          summary = firstParaToken.tokens.map(t => (t.text || t.raw || '')).join('');
        } else if (firstParaToken.raw) summary = String(firstParaToken.raw);
        else summary = String(firstParaToken.text || '');
      }

      // render markdown to HTML but inject IDs for headings
      const renderer = new marked.Renderer();
      // make external links open in new tab safely
      const originalLink = renderer.link;
      renderer.link = function(href, title, text) {
        const out = originalLink.call(this, href, title, text);
        if (/^https?:\/\//.test(href)) {
          return out.replace(/^<a /, '<a target="_blank" rel="noopener noreferrer" ');
        }
        return out;
      };
      renderer.heading = function(text, level){
        const id = slugify(text);
        return `<h${level} id="${id}">${text}</h${level}>`;
      };
      const html = marked.parser(tokens, {renderer, mangle:false});

      // assemble final layout: title, summary, toc, html
      reader.innerHTML = `\n        <div class=\"doc-hero\">\n          <h1>${f.title}</h1>\n          ${summary ? `<p class=\"doc-summary\">${summary}</p>` : ''}\n        </div>\n        <div class=\"doc-grid\">\n          <div class=\"doc-main\">${html}</div>\n          <aside class=\"doc-side\">${tocHtml}</aside>\n        </div>\n      `;

      titleEl.textContent = f.title;
      current = f;
      // highlight active item in sidebar
      document.querySelectorAll('.file-item').forEach(el => el.classList.toggle('active', el.dataset.path === f.path));
      // show/hide Edit button only if an editUrl is provided in the file entry
      if (openInEditor) {
        if (f.editUrl) {
          openInEditor.style.display = 'inline-block';
          openInEditor.href = f.editUrl;
        } else {
          openInEditor.style.display = 'none';
          openInEditor.removeAttribute('href');
        }
      }

      // enhance rendered HTML: callouts (>:note, >:warning) and code highlighting
      document.querySelectorAll('blockquote').forEach(b => {
        const t = b.textContent.trim().toLowerCase();
        if(t.startsWith('note:') || t.startsWith('nota:')) b.classList.add('admonition','note');
        if(t.startsWith('warning:') || t.startsWith('advertencia:')) b.classList.add('admonition','warning');
      });
      document.querySelectorAll('pre code').forEach(el => hljs.highlightElement(el));

      // add copy button to code blocks
      document.querySelectorAll('pre').forEach(pre => {
        if (pre.querySelector('.code-copy')) return;
        const btn = document.createElement('button');
        btn.className = 'code-copy';
        btn.type = 'button';
        btn.title = 'Copiar código';
        btn.textContent = 'Copiar';
        btn.addEventListener('click', async () => {
          const code = pre.querySelector('code');
          if (!code) return;
          try {
            await navigator.clipboard.writeText(code.innerText);
            btn.textContent = 'Copiado';
            setTimeout(() => btn.textContent = 'Copiar', 1400);
          } catch (e) {
            btn.textContent = 'Error';
            setTimeout(() => btn.textContent = 'Copiar', 1400);
          }
        });
        pre.style.position = 'relative';
        pre.appendChild(btn);
      });

      reader.scrollTop = 0;
    }catch(err){
      // user-friendly error message only (no debug logs in public project)
      reader.innerHTML = `
        <div style="color:#f88">Error cargando ${f.path}: ${err && err.message}</div>
        <div style="margin-top:8px;color:var(--muted)">Si estás abriendo el archivo localmente, sirve la carpeta <code>docs/</code> por HTTP (usa <code>serve-docs.cmd</code> o ` +
        `<code>python -m http.server --directory docs</code>).</div>
      `;
    }
  }

  async function init(){
    const r = await fetch(filesUrl);
    files = await r.json();
    renderSidebar(files);
    // load first
    if(files.length) loadFile(files[0]);
    // hook CTA buttons
    const openArchBtn = document.getElementById('openArchitecture');
    if(openArchBtn) openArchBtn.addEventListener('click', ()=>{
      const f = files.find(x => x.path === 'ARCHITECTURE.md');
      if(f) loadFile(f);
    });
    const openReadmeBtn = document.getElementById('openProjectReadme');
    if(openReadmeBtn) openReadmeBtn.addEventListener('click', ()=>{
      // try to load project README via relative path entry in _files.json if present
      const f = files.find(x => x.path.endsWith('README.md')) || files.find(x => x.path === '../README.md');
      if(f) loadFile(f); else window.open('../README.md', '_blank');
    });
  }

  search.addEventListener('input', (e)=>{
    const q = e.target.value.toLowerCase().trim();
    const filtered = files.filter(f => f.title.toLowerCase().includes(q) || (f.desc||'').toLowerCase().includes(q) || f.path.toLowerCase().includes(q));
    renderSidebar(filtered);
  });

  // Single dark theme only — remove theme toggle handling


  init();
})();
