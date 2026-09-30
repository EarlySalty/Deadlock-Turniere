import assert from 'node:assert/strict'
import test from 'node:test'
import { createElement } from 'react'
import { renderToString } from 'react-dom/server'

test('React und React DOM können gemeinsam rendern', () => {
  assert.equal(renderToString(createElement('strong', null, 'Turniere')), '<strong>Turniere</strong>')
})
