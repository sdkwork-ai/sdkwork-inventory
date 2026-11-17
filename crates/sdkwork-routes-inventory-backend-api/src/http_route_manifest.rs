use sdkwork_web_core::{HttpMethod, HttpRoute, HttpRouteManifest};

const HTTP_ROUTES: &[HttpRoute] = &[
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/inventory/stocks",
        "commerce.inventory.read",
        "inventory.stocks.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Patch,
        "/backend/v3/api/inventory/stocks/{stockId}",
        "commerce.inventory.manage",
        "inventory.stocks.update",
    ),
    HttpRoute::dual_token(
        HttpMethod::Post,
        "/backend/v3/api/inventory/stocks/{stockId}/adjustments",
        "commerce.inventory.manage",
        "inventory.stocks.adjustments.create",
    ),
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/inventory/reservations",
        "commerce.inventory.read",
        "inventory.reservations.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Get,
        "/backend/v3/api/inventory/movements",
        "commerce.inventory.read",
        "inventory.movements.list",
    ),
];

pub fn backend_route_manifest() -> HttpRouteManifest {
    HttpRouteManifest::new(HTTP_ROUTES)
}
