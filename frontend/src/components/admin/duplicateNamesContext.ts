import { createContext } from 'react'

export const DuplicateNamesContext = createContext<Set<string>>(new Set())
