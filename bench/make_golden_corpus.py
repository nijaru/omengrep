#!/usr/bin/env python3
import os
import shutil
from pathlib import Path

# Create directory
golden_dir = Path("bench/golden")
golden_dir.mkdir(parents=True, exist_ok=True)

# Copy existing
tests_golden = Path("crates/og/tests/golden")
for f in ["api_handlers.ts", "auth.py", "errors.rs", "server.go"]:
    src = tests_golden / f
    if src.exists():
        shutil.copy(src, golden_dir / f)

# Create remaining 16 files

files = {
    "database.rs": """
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::env;

pub async fn init_db_pool() -> Result<PgPool, sqlx::Error> {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .connect(&database_url)
        .await?;
    Ok(pool)
}
""",
    "workers.rs": """
use celery::Celery;
use log::{info, error};

#[celery::task]
pub async fn process_background_jobs(job_id: String, payload: String) -> celery::TaskResult<()> {
    info!("Processing background job: {}", job_id);
    // Simulate long running task
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    if payload.is_empty() {
        error!("Empty payload for job {}", job_id);
        return Err(celery::error::TaskError::ExpectedError("Empty payload".into()));
    }
    info!("Job {} completed", job_id);
    Ok(())
}
""",
    "utils.js": """
/**
 * Utility functions for string manipulation and formatting.
 */
export function capitalize(str) {
    if (!str) return '';
    return str.charAt(0).toUpperCase() + str.slice(1);
}

export function snakeToCamel(str) {
    return str.replace(/([-_][a-z])/g, group =>
        group.toUpperCase().replace('-', '').replace('_', '')
    );
}

export function chunkArray(array, size) {
    const result = [];
    for (let i = 0; i < array.length; i += size) {
        result.push(array.slice(i, i + size));
    }
    return result;
}
""",
    "models.py": """
from sqlalchemy import Column, Integer, String, Float, ForeignKey
from sqlalchemy.orm import declarative_base, relationship

Base = declarative_base()

class User(Base):
    __tablename__ = 'users'
    id = Column(Integer, primary_key=True)
    email = Column(String(255), unique=True, nullable=False)
    role = Column(String(50), default='user')
    orders = relationship("Order", back_populates="user")

class Order(Base):
    __tablename__ = 'orders'
    id = Column(Integer, primary_key=True)
    user_id = Column(Integer, ForeignKey('users.id'))
    total_amount = Column(Float, nullable=False)
    user = relationship("User", back_populates="orders")
""",
    "main.rs": """
mod database;
mod workers;
mod errors;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let pool = database::init_db_pool().await?;
    println!("Connected to DB");
    
    // Start HTTP server and workers...
    Ok(())
}
""",
    "config.go": """
package main

import (
	"os"
	"strconv"
)

type Config struct {
	Port int
	Env  string
}

func LoadConfig() Config {
	portStr := os.Getenv("PORT")
	port, _ := strconv.Atoi(portStr)
	if port == 0 {
		port = 8080
	}
	env := os.Getenv("APP_ENV")
	if env == "" {
		env = "development"
	}
	return Config{Port: port, Env: env}
}
""",
    "payments.ts": """
import Stripe from 'stripe';

const stripe = new Stripe(process.env.STRIPE_SECRET_KEY!, {
    apiVersion: '2023-10-16',
});

export async function createPaymentIntent(amount: number, currency: string) {
    return await stripe.paymentIntents.create({
        amount,
        currency,
        automatic_payment_methods: { enabled: true },
    });
}
""",
    "cache.py": """
import redis
import json
import os

redis_client = redis.Redis.from_url(os.getenv("REDIS_URL", "redis://localhost:6379/0"))

def get_cached_item(key: str):
    data = redis_client.get(key)
    if data:
        return json.loads(data)
    return None

def set_cached_item(key: str, value: dict, ttl: int = 3600):
    redis_client.setex(key, ttl, json.dumps(value))
""",
    "logger.go": """
package main

import (
	"go.uber.org/zap"
)

var Logger *zap.Logger

func InitLogger() {
	var err error
	Logger, err = zap.NewProduction()
	if err != nil {
		panic(err)
	}
	defer Logger.Sync()
}
""",
    "email.ts": """
import sgMail from '@sendgrid/mail';

sgMail.setApiKey(process.env.SENDGRID_API_KEY!);

export async function sendWelcomeEmail(to: string, name: string) {
    const msg = {
        to,
        from: 'noreply@myapp.com',
        subject: 'Welcome to MyApp!',
        text: `Hi ${name}, welcome to our platform!`,
        html: `<strong>Hi ${name}, welcome to our platform!</strong>`,
    };
    await sgMail.send(msg);
}
""",
    "cli.py": """
import argparse
import sys

def main():
    parser = argparse.ArgumentParser(description="MyApp Management CLI")
    parser.add_argument("--migrate", action="store_true", help="Run database migrations")
    parser.add_argument("--seed", action="store_true", help="Seed database with dummy data")
    args = parser.parse_args()

    if args.migrate:
        print("Running migrations...")
    elif args.seed:
        print("Seeding data...")
    else:
        parser.print_help()

if __name__ == "__main__":
    main()
""",
    "router.go": """
package main

import (
	"net/http"
	"github.com/gorilla/mux"
)

func SetupRouter() *mux.Router {
	r := mux.NewRouter()
	r.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.Write([]byte("OK"))
	}).Methods("GET")
	return r
}
""",
    "metrics.rs": """
use prometheus::{Counter, register_counter};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref HTTP_REQUESTS_TOTAL: Counter = register_counter!(
        "http_requests_total",
        "Total number of HTTP requests"
    ).unwrap();
}

pub fn track_request() {
    HTTP_REQUESTS_TOTAL.inc();
}
""",
    "validation.ts": """
import { z } from 'zod';

export const UserSchema = z.object({
    email: z.string().email(),
    password: z.string().min(8).max(100),
    age: z.number().int().positive().optional(),
});

export type UserInput = z.infer<typeof UserSchema>;
""",
    "queue.py": """
from celery import Celery
import os

app = Celery('myapp_tasks', broker=os.getenv('RABBITMQ_URL', 'amqp://guest@localhost//'))

@app.task
def send_async_email(email_address: str, subject: str):
    print(f"Sending email to {email_address} with subject: {subject}")
""",
    "security.go": """
package main

import (
	"golang.org/x/crypto/bcrypt"
)

func HashPassword(password string) (string, error) {
	bytes, err := bcrypt.GenerateFromPassword([]byte(password), 14)
	return string(bytes), err
}

func CheckPasswordHash(password, hash string) bool {
	err := bcrypt.CompareHashAndPassword([]byte(hash), []byte(password))
	return err == nil
}
"""
}

for name, content in files.items():
    with open(golden_dir / name, "w") as f:
        f.write(content.strip() + "\\n")

print(f"Created {len(files) + 4} files in {golden_dir}")
