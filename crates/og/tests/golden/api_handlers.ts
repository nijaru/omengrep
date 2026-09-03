/**
 * REST API handlers for user and product resources.
 */
import { Request, Response, NextFunction } from 'express';

interface User {
  id: string;
  email: string;
  name: string;
  role: 'admin' | 'user';
}

interface Product {
  id: string;
  name: string;
  price: number;
  stock: number;
}

// Database mock
const users: Map<string, User> = new Map();
const products: Map<string, Product> = new Map();

// Middleware for authentication
export function authMiddleware(req: Request, res: Response, next: NextFunction) {
  const token = req.headers.authorization?.replace('Bearer ', '');
  if (!token) {
    return res.status(401).json({ error: 'Unauthorized' });
  }
  // Validate token and attach user to request
  next();
}

// User handlers
export async function getUserById(req: Request, res: Response) {
  const { id } = req.params;
  const user = users.get(id);
  if (!user) {
    return res.status(404).json({ error: 'User not found' });
  }
  res.json(user);
}

export async function createUser(req: Request, res: Response) {
  const { email, name, role = 'user' } = req.body;
  if (!email || !name) {
    return res.status(400).json({ error: 'Email and name required' });
  }
  const id = crypto.randomUUID();
  const user: User = { id, email, name, role };
  users.set(id, user);
  res.status(201).json(user);
}

export async function updateUser(req: Request, res: Response) {
  const { id } = req.params;
  const user = users.get(id);
  if (!user) {
    return res.status(404).json({ error: 'User not found' });
  }
  const updated = { ...user, ...req.body, id };
  users.set(id, updated);
  res.json(updated);
}

export async function deleteUser(req: Request, res: Response) {
  const { id } = req.params;
  if (!users.has(id)) {
    return res.status(404).json({ error: 'User not found' });
  }
  users.delete(id);
  res.status(204).send();
}

// Product handlers
export async function listProducts(req: Request, res: Response) {
  const { minPrice, maxPrice, inStock } = req.query;
  let result = Array.from(products.values());

  if (minPrice) {
    result = result.filter(p => p.price >= Number(minPrice));
  }
  if (maxPrice) {
    result = result.filter(p => p.price <= Number(maxPrice));
  }
  if (inStock === 'true') {
    result = result.filter(p => p.stock > 0);
  }

  res.json(result);
}

export async function getProductById(req: Request, res: Response) {
  const { id } = req.params;
  const product = products.get(id);
  if (!product) {
    return res.status(404).json({ error: 'Product not found' });
  }
  res.json(product);
}

export async function createProduct(req: Request, res: Response) {
  const { name, price, stock = 0 } = req.body;
  if (!name || price === undefined) {
    return res.status(400).json({ error: 'Name and price required' });
  }
  const id = crypto.randomUUID();
  const product: Product = { id, name, price, stock };
  products.set(id, product);
  res.status(201).json(product);
}

export async function updateProductStock(req: Request, res: Response) {
  const { id } = req.params;
  const { quantity } = req.body;
  const product = products.get(id);
  if (!product) {
    return res.status(404).json({ error: 'Product not found' });
  }
  product.stock += quantity;
  if (product.stock < 0) {
    return res.status(400).json({ error: 'Insufficient stock' });
  }
  res.json(product);
}

// Error handler
export function errorHandler(err: Error, req: Request, res: Response, next: NextFunction) {
  console.error('API Error:', err.message);
  res.status(500).json({ error: 'Internal server error' });
}
