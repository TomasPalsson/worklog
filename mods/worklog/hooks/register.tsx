import type { Register } from 'claude-code'
import { registerCommand } from './command'
import { registerStatus } from './status'
import { registerTicket } from './ticket'

export const register: Register = on => {
  registerStatus(on)
  registerTicket(on)
  registerCommand(on)
}
