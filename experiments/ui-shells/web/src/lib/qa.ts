import { answerQuestion, type QaCitation } from '../data/wiki'

export interface QaMessage {
  id: number
  role: 'user' | 'assistant'
  text: string
  citations?: QaCitation[]
  degraded?: string
}

let nextId = 1

export function userMessage(text: string): QaMessage {
  return { id: nextId++, role: 'user', text }
}

export function assistantMessage(question: string): QaMessage {
  const result = answerQuestion(question)
  return {
    id: nextId++,
    role: 'assistant',
    text: result.answer,
    citations: result.citations,
    degraded: result.degraded,
  }
}

export const greeting: QaMessage = {
  id: nextId++,
  role: 'assistant',
  text: '本面板只基于知识库内页面作答，结论后附引用。当前为离线演示数据，不含真实模型调用。可以试问下方推荐问题，或直接用顶部搜索框提问。',
}
