import { createContext, useState, useEffect, useCallback } from 'react'

export const ClusterContext = createContext()

export function ClusterProvider({ children }) {
  const [clusters, setClusters] = useState([])
  const [nodes, setNodes] = useState([])
  const [templates, setTemplates] = useState([])
  const [config, setConfig] = useState(null)
  const [loading, setLoading] = useState(true)

  const fetchClusters = useCallback(async () => {
    try {
      const res = await fetch('/api/clusters')
      const data = await res.json()
      setClusters(data)
    } catch (e) { console.error(e) }
  }, [])

  const fetchNodes = useCallback(async () => {
    try {
      const res = await fetch('/api/nodes')
      const data = await res.json()
      setNodes(data)
    } catch (e) { console.error(e) }
  }, [])

  const fetchTemplates = useCallback(async () => {
    try {
      const res = await fetch('/api/templates')
      const data = await res.json()
      setTemplates(data)
    } catch (e) { console.error(e) }
  }, [])

  const fetchConfig = useCallback(async () => {
    try {
      const res = await fetch('/api/config')
      const data = await res.json()
      setConfig(data)
    } catch (e) { console.error(e) }
  }, [])

  useEffect(() => {
    Promise.all([fetchClusters, fetchNodes, fetchTemplates, fetchConfig]).finally(() => setLoading(false))
  }, [fetchClusters, fetchNodes, fetchTemplates, fetchConfig])

  return (
    <ClusterContext.Provider value={{ clusters, nodes, templates, config, loading, refresh: fetchClusters }}>
      {children}
    </ClusterContext.Provider>
  )
}
