import { NavLink, Outlet } from 'react-router-dom'

export function AppLayout() {
  return (
    <div className="app-shell">
      <header className="app-header">
        <span className="brand">Goodnight</span>
        <nav>
          <NavLink to="/">首页</NavLink>
          <NavLink to="/about">关于</NavLink>
        </nav>
      </header>
      <main>
        <Outlet />
      </main>
    </div>
  )
}
