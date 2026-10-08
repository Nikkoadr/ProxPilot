import { NavLink, useLocation } from 'react-router-dom'

export default function Layout({ children }) {
  const location = useLocation()

  return (
    <div className="app">
      <nav className="navbar">
        <NavLink to="/" className="navbar-brand">
          <div className="logo">⚡</div>
          <span>Proxmox Panel</span>
        </NavLink>
        <div className="nav-links">
          <NavLink to="/" className={({ isActive }) => `nav-link ${isActive ? 'active' : ''}`}>
            Dashboard
          </NavLink>
          <NavLink to="/new" className={({ isActive }) => `nav-link ${isActive ? 'active' : ''}`}>
            New Cluster
          </NavLink>
        </div>
      </nav>
      <main className="main">
        {children}
      </main>
    </div>
  )
}
