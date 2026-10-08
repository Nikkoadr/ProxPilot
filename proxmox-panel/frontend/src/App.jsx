import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom'
import { ClusterProvider } from './context/ClusterContext'
import Layout from './components/Layout'
import Dashboard from './pages/Dashboard'
import NewCluster from './pages/NewCluster'
import ClusterDetail from './pages/ClusterDetail'

export default function App() {
  return (
    <BrowserRouter>
      <ClusterProvider>
        <Layout>
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/new" element={<NewCluster />} />
            <Route path="/cluster/:id" element={<ClusterDetail />} />
          </Routes>
        </Layout>
      </ClusterProvider>
    </BrowserRouter>
  )
}
