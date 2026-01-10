import React, { useState } from 'react'
import { QueryMetadata } from '../queryDiscovery'
import './QueryForm.css'

interface Client {
  id: string
  name: string
  connected: boolean
}

interface QueryFormProps {
  queries: QueryMetadata[]
  onSubmit: (queryId: string, params: Record<string, any>) => void
  selectedClient: Client | undefined
}

export default function QueryForm({
  queries,
  onSubmit,
  selectedClient,
}: QueryFormProps) {
  const [selectedQueryId, setSelectedQueryId] = useState<string | null>(null)
  const [queryParams, setQueryParams] = useState<Record<string, any>>({})
  const [isSubmitting, setIsSubmitting] = useState(false)

  const selectedQuery = queries.find((q) => q.id === selectedQueryId)

  const handleQuerySelect = (queryId: string) => {
    setSelectedQueryId(queryId)
    const query = queries.find((q) => q.id === queryId)
    const initialParams: Record<string, any> = {}
    
    // Initialize boolean fields to false
    if (query?.inputFields) {
      query.inputFields.forEach(field => {
        if (field.type === 'boolean') {
          initialParams[field.name] = false
        }
      })
    }
    
    setQueryParams(initialParams)
  }

  const handleParamChange = (field: string, value: any, fieldType?: 'string' | 'number' | 'boolean') => {
    let convertedValue: any = value;
    
    if (fieldType === 'boolean') {
      // For checkboxes, value is already boolean
      convertedValue = value;
    } else if (fieldType === 'number') {
      // Convert to number, or keep as empty string if invalid
      const num = Number(value);
      convertedValue = value === '' ? '' : (isNaN(num) ? value : num);
    } else {
      // String type - keep as is
      convertedValue = value;
    }
    
    setQueryParams((prev) => ({ ...prev, [field]: convertedValue }))
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!selectedQueryId || !selectedClient?.connected) return

    // Ensure boolean fields have explicit true/false values
    const paramsToSubmit: Record<string, any> = { ...queryParams };
    if (selectedQuery?.inputFields) {
      selectedQuery.inputFields.forEach(field => {
        if (field.type === 'boolean' && paramsToSubmit[field.name] === undefined) {
          paramsToSubmit[field.name] = false;
        }
      });
    }

    setIsSubmitting(true)
    try {
      await onSubmit(selectedQueryId, paramsToSubmit)
    } finally {
      setIsSubmitting(false)
    }
  }

  if (!selectedClient) {
    return (
      <div className="query-form">
        <p className="no-client">Select a client to run queries</p>
      </div>
    )
  }

  if (!selectedClient.connected) {
    return (
      <div className="query-form">
        <p className="no-client">Client not connected</p>
      </div>
    )
  }

  return (
    <div className="query-form">
      <h2>Query Form</h2>
      <form onSubmit={handleSubmit}>
        <div className="form-group">
          <label htmlFor="query-select">Select Query:</label>
          <select
            id="query-select"
            value={selectedQueryId || ''}
            onChange={(e) => handleQuerySelect(e.target.value)}
            className="query-select"
          >
            <option value="">-- Select a query --</option>
            {queries.map((query, index) => (
              <option key={`${query.id}-${index}`} value={query.id}>
                {query.name}
              </option>
            ))}
          </select>
        </div>

        {selectedQuery && (
          <div className="form-params">
            {selectedQuery.inputFields && selectedQuery.inputFields.length > 0 ? (
              selectedQuery.inputFields.map((field, index) => {
                const fieldValue = queryParams[field.name];
                const isBoolean = field.type === 'boolean';
                const isNumber = field.type === 'number';
                
                return (
                  <div key={`${field.name}-${index}`} className="form-group">
                    {isBoolean ? (
                      <>
                        <label htmlFor={`param-${field.name}-${index}`} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          <input
                            id={`param-${field.name}-${index}`}
                            type="checkbox"
                            checked={fieldValue === true}
                            onChange={(e) => handleParamChange(field.name, e.target.checked, field.type)}
                            className="param-checkbox"
                          />
                          <span>{field.name}</span>
                        </label>
                      </>
                    ) : (
                      <>
                        <label htmlFor={`param-${field.name}-${index}`}>{field.name}:</label>
                        <input
                          id={`param-${field.name}-${index}`}
                          type={isNumber ? 'number' : 'text'}
                          value={fieldValue !== undefined ? String(fieldValue) : ''}
                          onChange={(e) => handleParamChange(field.name, e.target.value, field.type)}
                          placeholder={`Enter ${field.name}...`}
                          className="param-input"
                        />
                      </>
                    )}
                  </div>
                );
              })
            ) : (
              <p className="no-params">No parameters required</p>
            )}
          </div>
        )}

        <button
          type="submit"
          disabled={!selectedQueryId || isSubmitting}
          className="submit-btn"
        >
          {isSubmitting ? 'Submitting...' : 'Submit Query'}
        </button>
      </form>
    </div>
  )
}
