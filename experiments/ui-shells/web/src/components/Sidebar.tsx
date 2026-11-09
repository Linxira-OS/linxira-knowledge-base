import { NavLink } from 'react-router-dom'
import { pagesOf, spaces } from '../data/wiki'

export function Sidebar() {
  return (
    <aside className="hidden w-60 shrink-0 flex-col border-r border-line bg-panel lg:flex">
      <div className="flex items-center justify-between px-4 pb-1.5 pt-3.5">
        <span className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-fg-3">空间</span>
        <span className="rounded-md border border-line px-1.5 py-0.5 text-[10px] text-fg-3">solo</span>
      </div>
      <nav className="thin-scroll flex-1 overflow-y-auto px-2 pb-4">
        {spaces.map((space) => (
          <div key={space.id} className="mb-4">
            <div className="flex items-center gap-2 px-2 py-1.5 text-[12px] font-medium text-fg-2">
              <span className="text-[10px] text-fg-3">{space.glyph}</span>
              {space.name}
            </div>
            <ul className="space-y-0.5">
              {pagesOf(space.id).map((page) => (
                <li key={page.id}>
                  <NavLink
                    to={`/p/${page.id}`}
                    className={({ isActive }) =>
                      `block truncate rounded-lg px-2.5 py-[7px] text-[13px] transition-colors ${
                        isActive
                          ? 'bg-accent-weak font-medium text-accent'
                          : 'text-fg-2 hover:bg-raise hover:text-fg'
                      }`
                    }
                  >
                    {page.title}
                  </NavLink>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </nav>
      <div className="border-t border-line px-4 py-3">
        <div className="flex items-center gap-2 text-[11px] text-fg-3">
          <span className="h-1.5 w-1.5 rounded-full bg-accent" />
          离线数据 · 7 篇 · 0 外部请求
        </div>
        <div className="mt-1 text-[10.5px] text-fg-3">索引：SQLite FTS5（模拟）</div>
      </div>
    </aside>
  )
}
