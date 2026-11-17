#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const generatorBin = path.resolve(root, '..', 'sdkwork-sdk-generator', 'bin', 'sdkgen.js');
const checkMode = process.argv.includes('--check');
const OWNER = 'sdkwork-inventory';

const pageParameters = [
  { name: 'page', in: 'query', required: false, schema: { type: 'integer', minimum: 1, default: 1 } },
  { name: 'page_size', in: 'query', required: false, schema: { type: 'integer', minimum: 1, maximum: 200, default: 20 } },
];

const targets = [
  {
    surface: 'app-api',
    sdkType: 'app',
    authority: 'sdkwork-inventory-app-api',
    family: 'sdkwork-inventory-app-sdk',
    packageName: 'sdkwork-routes-inventory-app-api',
    crateImport: 'sdkwork_routes_inventory_app_api',
    operations: [
      {
        method: 'get', path: '/app/v3/api/shops/current/inventory/stocks',
        operationId: 'inventory.stocks.list', permission: 'commerce.inventory.read',
        parameters: pageParameters, response: 'InventoryStockPageResponse',
        summary: 'List the current shop inventory stocks.',
      },
      {
        method: 'post', path: '/app/v3/api/shops/current/inventory/stocks/{stockId}/adjustments',
        operationId: 'inventory.stocks.adjustments.create', permission: 'commerce.inventory.manage',
        parameters: [{ name: 'stockId', in: 'path', required: true, schema: { type: 'string', minLength: 1 } }],
        body: 'CreateInventoryAdjustmentRequest', response: 'InventoryStockResourceResponse', status: '201',
        summary: 'Create an inventory stock adjustment for the current shop.',
      },
    ],
  },
  {
    surface: 'backend-api',
    sdkType: 'backend',
    authority: 'sdkwork-inventory-backend-api',
    family: 'sdkwork-inventory-backend-sdk',
    packageName: 'sdkwork-routes-inventory-backend-api',
    crateImport: 'sdkwork_routes_inventory_backend_api',
    operations: [
      {
        method: 'get', path: '/backend/v3/api/inventory/stocks',
        operationId: 'inventory.stocks.list', permission: 'commerce.inventory.read',
        parameters: [
          { name: 'sku_id', in: 'query', required: false, schema: { type: 'string' } },
          { name: 'warehouse_id', in: 'query', required: false, schema: { type: 'string' } },
          { name: 'status', in: 'query', required: false, schema: { type: 'string' } },
          ...pageParameters,
        ],
        response: 'InventoryStockPageResponse', summary: 'List inventory stocks for operators.',
      },
      {
        method: 'patch', path: '/backend/v3/api/inventory/stocks/{stockId}',
        operationId: 'inventory.stocks.update', permission: 'commerce.inventory.manage',
        parameters: [{ name: 'stockId', in: 'path', required: true, schema: { type: 'string', minLength: 1 } }],
        body: 'UpdateInventoryStockRequest', response: 'InventoryStockResourceResponse',
        summary: 'Update an inventory stock record.',
      },
      {
        method: 'post', path: '/backend/v3/api/inventory/stocks/{stockId}/adjustments',
        operationId: 'inventory.stocks.adjustments.create', permission: 'commerce.inventory.manage',
        idempotent: true,
        parameters: [
          { name: 'stockId', in: 'path', required: true, schema: { type: 'string', minLength: 1 } },
          {
            name: 'Idempotency-Key',
            in: 'header',
            required: true,
            schema: { type: 'string', minLength: 1, maxLength: 128 },
          },
        ],
        body: 'AdjustInventoryStockRequest', response: 'InventoryStockResourceResponse', status: '201',
        summary: 'Adjust an inventory stock record and append a movement.',
      },
      {
        method: 'get', path: '/backend/v3/api/inventory/reservations',
        operationId: 'inventory.reservations.list', permission: 'commerce.inventory.read',
        parameters: [
          { name: 'order_id', in: 'query', required: false, schema: { type: 'string' } },
          { name: 'sku_id', in: 'query', required: false, schema: { type: 'string' } },
          { name: 'status', in: 'query', required: false, schema: { type: 'string' } },
          ...pageParameters,
        ],
        response: 'InventoryReservationPageResponse', summary: 'List inventory reservations.',
      },
      {
        method: 'get', path: '/backend/v3/api/inventory/movements',
        operationId: 'inventory.movements.list', permission: 'commerce.inventory.read',
        parameters: [
          { name: 'sku_id', in: 'query', required: false, schema: { type: 'string' } },
          { name: 'movement_type', in: 'query', required: false, schema: { type: 'string' } },
          ...pageParameters,
        ],
        response: 'InventoryMovementPageResponse', summary: 'List inventory movements.',
      },
    ],
  },
];

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function pageResponse(itemRef) {
  return {
    type: 'object', required: ['code', 'data', 'traceId'],
    properties: {
      code: { type: 'integer', format: 'int32', const: 0 },
      data: {
        type: 'object', required: ['items', 'pageInfo'],
        properties: {
          items: { type: 'array', items: { $ref: itemRef } },
          pageInfo: { $ref: '#/components/schemas/PageInfo' },
        },
      },
      traceId: { type: 'string', format: 'uuid' },
    },
  };
}

function resourceResponse(itemRef) {
  return {
    type: 'object', required: ['code', 'data', 'traceId'],
    properties: {
      code: { type: 'integer', format: 'int32', const: 0 },
      data: { type: 'object', required: ['item'], properties: { item: { $ref: itemRef } } },
      traceId: { type: 'string', format: 'uuid' },
    },
  };
}

function schemas() {
  const int64 = () => ({
    type: 'string',
    format: 'int64',
    pattern: '^-?[0-9]+$',
    'x-sdkwork-int64-string': true,
  });
  const recordIdentity = {
    id: { type: 'string' }, tenantId: { type: 'string' },
    organizationId: { type: ['string', 'null'] }, createdAt: { type: 'string' }, updatedAt: { type: 'string' },
  };
  return {
    SdkWorkApiResponse: {
      type: 'object', required: ['code', 'data', 'traceId'],
      properties: {
        code: { type: 'integer', format: 'int32', const: 0 },
        data: {},
        traceId: { type: 'string', format: 'uuid' },
      },
    },
    PageInfo: {
      type: 'object', required: ['mode'],
      properties: {
        mode: { type: 'string', enum: ['offset', 'cursor'] }, page: { type: ['integer', 'null'] },
        pageSize: { type: ['integer', 'null'] }, totalItems: { type: ['string', 'null'] },
        totalPages: { type: ['integer', 'null'] }, nextCursor: { type: ['string', 'null'] },
        hasMore: { type: ['boolean', 'null'] },
      },
    },
    ProblemDetail: {
      type: 'object', required: ['type', 'title', 'status', 'detail', 'code', 'traceId'],
      properties: {
        type: { type: 'string', format: 'uri-reference' }, title: { type: 'string' },
        status: { type: 'integer', format: 'int32' }, detail: { type: 'string' },
        code: { type: 'integer', format: 'int32' }, traceId: { type: 'string', format: 'uuid' },
        instance: { type: ['string', 'null'] }, operationId: { type: ['string', 'null'] },
      },
    },
    InventoryStock: {
      type: 'object', required: ['id', 'tenantId', 'skuId', 'availableQuantity', 'reservedQuantity', 'status'],
      properties: {
        ...recordIdentity, skuId: { type: 'string' }, warehouseId: { type: ['string', 'null'] },
        fulfillmentNodeId: { type: 'string' }, onHandQuantity: int64(),
        availableQuantity: int64(), lockedQuantity: int64(),
        reservedQuantity: int64(), inboundQuantity: int64(),
        damagedQuantity: int64(), safetyStockQuantity: int64(),
        version: int64(), status: { type: 'string' },
      }, additionalProperties: true,
    },
    InventoryReservation: {
      type: 'object', required: ['id', 'tenantId', 'skuId', 'status', 'quantity'],
      properties: {
        ...recordIdentity, reservationNo: { type: 'string' }, orderId: { type: ['string', 'null'] },
        skuId: { type: 'string' }, status: { type: 'string' }, quantity: int64(),
      }, additionalProperties: true,
    },
    InventoryMovement: {
      type: 'object', required: ['id', 'tenantId', 'skuId', 'movementType', 'quantity'],
      properties: {
        ...recordIdentity, movementNo: { type: 'string' }, skuId: { type: 'string' },
        movementType: { type: 'string' }, quantity: int64(),
        direction: { type: 'string' }, occurredAt: { type: 'string' },
      }, additionalProperties: true,
    },
    CreateInventoryAdjustmentRequest: {
      type: 'object', required: ['quantityDelta'],
      properties: { quantityDelta: int64() }, additionalProperties: false,
    },
    UpdateInventoryStockRequest: {
      type: 'object', minProperties: 1,
      properties: {
        availableQuantity: int64(),
        safetyStockQuantity: int64(), status: { type: 'string' },
      }, additionalProperties: false,
    },
    AdjustInventoryStockRequest: {
      type: 'object', required: ['quantityDelta'],
      properties: {
        quantityDelta: {
          type: 'string', format: 'int64', pattern: '^-?[0-9]+$', 'x-sdkwork-int64-string': true,
          description: 'Signed quantity delta; positive records an inbound movement, negative an outbound one.',
        },
        movementType: { type: 'string', enum: ['inbound', 'outbound', 'stocktake'], default: 'stocktake' },
        reason: { type: 'string' },
        requestNo: { type: 'string' },
      }, additionalProperties: false,
    },
    InventoryStockPageResponse: pageResponse('#/components/schemas/InventoryStock'),
    InventoryReservationPageResponse: pageResponse('#/components/schemas/InventoryReservation'),
    InventoryMovementPageResponse: pageResponse('#/components/schemas/InventoryMovement'),
    InventoryStockResourceResponse: resourceResponse('#/components/schemas/InventoryStock'),
  };
}

function operationObject(target, entry) {
  const successStatus = entry.status ?? '200';
  const problemResponse = (description) => ({
    description,
    content: { 'application/problem+json': { schema: { $ref: '#/components/schemas/ProblemDetail' } } },
  });
  const result = {
    tags: ['inventory'], summary: entry.summary, operationId: entry.operationId,
    parameters: entry.parameters ?? [],
    responses: {
      [successStatus]: {
        description: successStatus === '201' ? 'Created' : 'Successful response',
        content: { 'application/json': { schema: { $ref: `#/components/schemas/${entry.response}` } } },
      },
      '400': problemResponse('Invalid request'),
      '401': problemResponse('Authentication required'),
      '403': problemResponse('Permission denied'),
      '404': problemResponse('Resource not found'),
      '409': problemResponse('Resource conflict'),
      '500': problemResponse('Internal server error'),
    },
    security: [{ AuthToken: [], AccessToken: [] }],
    'x-sdkwork-owner': OWNER,
    'x-sdkwork-api-authority': target.authority,
    'x-sdkwork-domain': 'commerce',
    'x-sdkwork-resource': entry.operationId.split('.').slice(0, -1).join('.'),
    'x-sdkwork-request-context': 'WebRequestContext',
    'x-sdkwork-api-surface': target.surface,
    'x-sdkwork-server-request-id': true,
    'x-sdkwork-source-route-crate': target.packageName,
    'x-sdkwork-source': `${target.packageName}:${entry.path}`,
    'x-sdkwork-auth-mode': 'dual-token',
    'x-sdkwork-permission': entry.permission,
  };
  if (entry.body) {
    result.requestBody = {
      required: true,
      content: { 'application/json': { schema: { $ref: `#/components/schemas/${entry.body}` } } },
    };
  }
  if (entry.idempotent) {
    result['x-sdkwork-idempotent'] = true;
  }
  return result;
}

function document(target) {
  const paths = {};
  for (const entry of target.operations) {
    paths[entry.path] ??= {};
    paths[entry.path][entry.method] = operationObject(target, entry);
  }
  const problemResponse = (description) => ({
    description,
    content: { 'application/problem+json': { schema: { $ref: '#/components/schemas/ProblemDetail' } } },
  });
  return {
    openapi: '3.1.2',
    info: {
      title: `SDKWork Inventory ${target.surface === 'app-api' ? 'App' : 'Backend'} API`,
      version: '1.0.0',
      description: 'Owner-only inventory capability API contract.',
      'x-sdkwork-api-authority': target.authority,
      'x-sdkwork-sdk-family': target.family,
      'x-sdkwork-owner': OWNER,
    },
    servers: [{ url: 'http://127.0.0.1:8080', description: 'Local SDKWork gateway' }],
    tags: [{ name: 'inventory', description: 'Inventory capability resources.', 'x-sdk-nested-resource-surface': true }],
    security: [{ AuthToken: [], AccessToken: [] }],
    paths,
    components: {
      securitySchemes: {
        AuthToken: { type: 'http', scheme: 'bearer', bearerFormat: 'JWT' },
        AccessToken: { type: 'apiKey', in: 'header', name: 'Access-Token' },
      },
      schemas: schemas(),
      responses: {
        BadRequest: problemResponse('Invalid request'), Unauthorized: problemResponse('Authentication required'),
        Forbidden: problemResponse('Permission denied'), NotFound: problemResponse('Resource not found'),
        Conflict: problemResponse('Resource conflict'), InternalError: problemResponse('Internal server error'),
      },
    },
  };
}

function routeManifest(target) {
  return {
    schemaVersion: 1, kind: 'sdkwork.route.manifest', packageName: target.packageName,
    surface: target.surface, owner: OWNER, domain: 'commerce', capability: 'inventory',
    apiAuthority: target.authority, sdkFamily: target.family,
    prefix: target.surface === 'app-api' ? '/app/v3/api' : '/backend/v3/api',
    source: { crateRoot: `crates/${target.packageName}`, crateImport: target.crateImport },
    routes: target.operations.map((entry) => ({
      method: entry.method.toUpperCase(), path: entry.path, operationId: entry.operationId,
      tags: ['inventory'], auth: { mode: 'dual-token', required: true, permission: entry.permission },
      ownership: { owner: OWNER, apiAuthority: target.authority },
      source: { file: `${target.packageName}:${entry.path}` },
      requestContext: 'WebRequestContext', apiSurface: target.surface,
    })),
  };
}

function synchronize(relativePath, content) {
  const targetPath = path.join(root, relativePath);
  const current = existsSync(targetPath) ? readFileSync(targetPath, 'utf8') : '';
  if (checkMode && current !== content) throw new Error(`${relativePath} is not synchronized`);
  if (!checkMode && current !== content) {
    mkdirSync(path.dirname(targetPath), { recursive: true });
    writeFileSync(targetPath, content, 'utf8');
  }
}

function generate(target) {
  const openapi = stableJson(document(target));
  const familyRoot = path.join(root, 'sdks', target.family);
  const generatedRoot = path.join(familyRoot, `${target.family}-typescript`, 'generated', 'server-openapi');
  synchronize(`apis/${target.surface}/inventory/${target.authority}.openapi.json`, openapi);
  synchronize(`sdks/${target.family}/openapi/${target.authority}.openapi.json`, openapi);
  synchronize(`sdks/${target.family}/openapi/${target.authority}.sdkgen.json`, openapi);
  synchronize(`sdks/_route-manifests/${target.surface}/${target.packageName}.route-manifest.json`, stableJson(routeManifest(target)));
  const manifest = JSON.parse(readFileSync(path.join(familyRoot, 'sdk-manifest.json'), 'utf8'));
  if (manifest.sdkOwner !== OWNER || manifest.apiAuthority !== target.authority
      || manifest.ownerOnlyOperationCount !== target.operations.length) {
    throw new Error(`${target.family}/sdk-manifest.json does not match the authority contract`);
  }
  if (!checkMode) {
    const sdkgenPath = path.join(familyRoot, 'openapi', `${target.authority}.sdkgen.json`);
    const result = spawnSync('node', [
      generatorBin, 'generate', '--input', sdkgenPath, '--output', generatedRoot,
      '--name', target.family, '--type', target.sdkType, '--language', 'typescript',
      '--base-url', 'http://127.0.0.1:8080',
      '--api-prefix', target.surface === 'app-api' ? '/app/v3/api' : '/backend/v3/api',
      '--fixed-sdk-version', '0.1.0', '--sdk-root', familyRoot, '--sdk-name', target.family,
      '--package-name', `${target.family}-generated-typescript`, '--standard-profile', 'sdkwork-v3',
    ], { cwd: familyRoot, stdio: 'inherit' });
    if (result.status !== 0) throw new Error(`${target.family} sdkgen failed with exit code ${result.status}`);
  } else if (!existsSync(path.join(generatedRoot, 'src', 'index.ts'))) {
    throw new Error(`${target.family} generated TypeScript transport is missing`);
  }
  return `${target.family}:${target.operations.length}`;
}

try {
  const result = targets.map(generate);
  process.stdout.write(`[inventory_api_sdk_generate] ${checkMode ? 'check passed' : 'generation completed'} (${result.join(', ')})\n`);
} catch (error) {
  process.stderr.write(`[inventory_api_sdk_generate] ${error instanceof Error ? error.message : String(error)}\n`);
  process.exit(1);
}
