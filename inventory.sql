CREATE TABLE categories (
    id SERIAL PRIMARY KEY,
    name TEXT UNIQUE NOT NULL
);

CREATE TABLE footprints (
    id SERIAL PRIMARY KEY,
    name TEXT UNIQUE NOT NULL
);

CREATE TABLE parts (
    id SERIAL PRIMARY KEY,
    category_id INTEGER NOT NULL REFERENCES categories(id),
    footprint_id INTEGER REFERENCES footprints(id),
    
    mpn TEXT UNIQUE,            
    
    value REAL,   
    volt_rating REAL,
    watt_rating REAL,
    amp_rating REAL,
    percent_tol REAL,
    stats TEXT,
    comments TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE locations (
    id SERIAL PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT
);

CREATE TABLE stock (
    id SERIAL PRIMARY KEY,
    part_id INTEGER NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
    location_id INTEGER REFERENCES locations(id) ON DELETE RESTRICT,
    
    quantity INTEGER DEFAULT 0 CHECK (quantity >= 0),
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    staged INTEGER CHECK (staged >= 0),

    UNIQUE(part_id, location_id)
);

CREATE INDEX idx_stock_part_id ON stock(part_id);
CREATE INDEX idx_stock_location_id ON stock(location_id);
CREATE INDEX idx_parts_category_id ON parts(category_id);
CREATE INDEX idx_parts_footprint_id ON parts(footprint_id);

CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER stock_updated_at
    BEFORE UPDATE ON stock
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

CREATE VIEW inventory AS
SELECT
    p.id,
    p.mpn,
    c.name AS category,
    f.name AS footprint,
    p.value,
    l.name AS location,
    s.quantity,
    s.staged,
    p.comments
FROM parts p
LEFT JOIN stock s ON p.id = s.part_id
LEFT JOIN locations l ON s.location_id = l.id
LEFT JOIN categories c ON p.category_id = c.id
LEFT JOIN footprints f ON p.footprint_id = f.id;
